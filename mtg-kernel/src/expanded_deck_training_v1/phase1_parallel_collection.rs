//! Parallelize only independent episodes from one fixed collecting checkpoint.
//! Each worker owns its model, inference scratch, opponent cache and game/RNG
//! state. Only work indices are shared. Published trajectories and the learner
//! update keep the original schedule order, regardless of completion order.

use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

pub(crate) fn validate_collection_workers_v1(workers: usize) -> Result<(), String> {
    ensure(
        (1..=1024).contains(&workers),
        "collection workers must be in 1..=1024",
    )
}

#[derive(Debug, Serialize)]
pub(super) struct WorkerTimingV1 {
    worker: usize,
    completed_episodes: usize,
    busy_seconds: f64,
    /// Schedule indices, including failed attempts inside each job's span.
    /// Reuses the worker clock already read for busy_seconds.
    episode_seconds: Vec<(usize, f64)>,
}

struct CollectorV1 {
    policy: FrozenPlayPolicyV1,
    opponent_cache: OpponentCacheV1,
}

/// Bounded dynamic scheduling with no shared model lock and no worker left
/// alive on return. A failed job or panic cancels new dispatch; already active
/// jobs join before the caller can attempt an update or publish completion.
/// A worker that finds no job left calls `on_idle` once before it stops.
pub(super) fn ordered_parallel_jobs_v1<W, T, F, I>(
    contexts: Vec<W>,
    job_count: usize,
    work: F,
    on_idle: I,
) -> Result<(Vec<T>, Vec<WorkerTimingV1>), String>
where
    W: Send,
    T: Send,
    F: Fn(&mut W, usize) -> Result<T, String> + Sync,
    I: Fn() + Sync,
{
    ensure(
        !contexts.is_empty() && contexts.len() <= job_count,
        "invalid parallel job dimensions",
    )?;
    let next = AtomicUsize::new(0);
    let cancelled = AtomicBool::new(false);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(contexts.len());
        let mut failure = None;
        for (worker, mut context) in contexts.into_iter().enumerate() {
            let next = &next;
            let cancelled = &cancelled;
            let work = &work;
            let on_idle = &on_idle;
            let spawned = thread::Builder::new()
                .name(format!("expanded-collector-{worker}"))
                .stack_size(8 * 1024 * 1024)
                .spawn_scoped(scope, move || {
                    let mut results = Vec::new();
                    let mut busy_seconds = 0.0;
                    let mut episode_seconds = Vec::new();
                    while !cancelled.load(Ordering::Acquire) {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= job_count || cancelled.load(Ordering::Acquire) {
                            if index >= job_count {
                                on_idle();
                            }
                            break;
                        }
                        let started = Instant::now();
                        let result = catch_unwind(AssertUnwindSafe(|| work(&mut context, index)));
                        let seconds = started.elapsed().as_secs_f64();
                        busy_seconds += seconds;
                        episode_seconds.push((index, seconds));
                        match result {
                            Ok(Ok(value)) => results.push((index, value)),
                            Ok(Err(error)) => {
                                cancelled.store(true, Ordering::Release);
                                return Err(format!(
                                    "collector {worker}, episode {index}: {error}"
                                ));
                            }
                            Err(_) => {
                                cancelled.store(true, Ordering::Release);
                                return Err(format!(
                                    "collector {worker}, episode {index}: worker panicked"
                                ));
                            }
                        }
                    }
                    let timing = WorkerTimingV1 {
                        worker,
                        completed_episodes: results.len(),
                        busy_seconds,
                        episode_seconds,
                    };
                    Ok((results, timing))
                });
            match spawned {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    cancelled.store(true, Ordering::Release);
                    failure = Some(format!("could not start collector {worker}: {error}"));
                    break;
                }
            }
        }
        let mut ordered: Vec<Option<T>> = (0..job_count).map(|_| None).collect();
        let mut timings = Vec::with_capacity(handles.len());
        // Join every child even after an error. No collector outlives this
        // call; `on_idle` may leave parse-ahead helpers running, which publish
        // nothing and only hand exact parses to later readers.
        for handle in handles {
            match handle.join() {
                Ok(Ok((results, timing))) => {
                    timings.push(timing);
                    for (index, value) in results {
                        if ordered[index].replace(value).is_some() && failure.is_none() {
                            failure = Some("collector produced a duplicate job index".into());
                        }
                    }
                }
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    cancelled.store(true, Ordering::Release);
                    failure.get_or_insert_with(|| "collector thread panicked".into());
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        let outputs = ordered
            .into_iter()
            .enumerate()
            .map(|(index, result)| result.ok_or_else(|| format!("missing episode {index}")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((outputs, timings))
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the collection command's fields, plus EX1's hook"
)]
pub(super) fn collect_parallel_v1(
    source: ExpandedModelSourceV1,
    episodes: Vec<ExpandedEpisodeV1>,
    workers: usize,
    max_non_natural_episode_fraction: f32,
    collection_sampler: CollectionSamplerV1,
    training_math: TrainingMathV1,
    ex1: Option<super::ex1_collect_v1::Ex1CollectOptionsV1>,
    output_directory: PathBuf,
) -> Result<Value, String> {
    let started = Instant::now();
    validate_collection_workers_v1(workers)?;
    ensure(workers >= 2, "use collect for a single collection worker")?;
    ensure(
        !episodes.is_empty() && episodes.len() <= 1024,
        "invalid collection size",
    )?;
    validate_max_non_natural_episode_fraction_v1(max_non_natural_episode_fraction)?;
    let mut ids = BTreeSet::new();
    for episode in &episodes {
        episode.configurations()?;
        ensure(ids.insert(&episode.id), "duplicate episode id")?;
    }
    ensure(
        !output_directory.exists(),
        "output directory already exists",
    )?;
    let (mut policy, mut state, transfer) = initialize_with_transfer_context(&source)?;
    bind_training_math_v1(
        &source,
        transfer.as_ref(),
        training_math,
        &mut policy,
        &mut state,
    )?;
    collection_sampler.apply_to_learner_v1(&mut policy);
    if let Some(context) = &transfer {
        context.validate_batch(&episodes)?;
    }
    let state_hash = hex(&state.state_sha256_v1().map_err(err)?);
    let learner = ExpandedSeatBehaviorV1 {
        source: source.clone(),
        identity: inference_identity_with_state_sha256_v1(
            &source,
            &policy,
            &state,
            state_hash.clone(),
        ),
    };
    let input_contract = no_state_digest_v1::policy_contract_record_v1(&policy)?;
    drop(state);
    let verified_sources = verify_collection_opponent_sources_v1(&episodes)?;
    let worker_count = workers.min(episodes.len());
    let contexts = (0..worker_count)
        .map(|_| {
            Ok(CollectorV1 {
                policy: policy.fork_for_collection_v3()?,
                opponent_cache: OpponentCacheV1::for_collection(verified_sources.clone())
                    .with_training_math_v1(training_math),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    drop(policy);
    let initialization_seconds = started.elapsed().as_secs_f64();
    fs::create_dir(&output_directory).map_err(err)?;
    // Side channel, independent of `ordered_parallel_jobs_v1`'s own success/
    // failure return: a worker extends this the moment it has an attempt's
    // ledger entries, whether or not that worker (or a sibling) ultimately
    // fails, so the ledger is preserved even when the whole command aborts
    // (a slot exhausting `NON_NATURAL_RETRY_LIMIT`, which
    // `ordered_parallel_jobs_v1` turns into a fatal `Err` that discards
    // every worker's normal `T` return value).
    let ledger: Mutex<Vec<NonNaturalLedgerEntryV1>> = Mutex::new(Vec::new());
    let collection_result = ordered_parallel_jobs_v1(
        contexts,
        episodes.len(),
        |collector, index| {
            let episode = &episodes[index];
            eprintln!(
                "collect parallel episode {}/{} {}",
                index + 1,
                episodes.len(),
                episode.id
            );
            // Current-self opponents need a private policy, but not another
            // disk parse and validation of the same learner checkpoint.
            if episode.opponent.as_ref() == Some(&source)
                && !collector.opponent_cache.contains(&source)
            {
                // A frozen opponent keeps its production sampler even when
                // it shares the learner's source (for example `initial`
                // at the first update), exactly as the serial loader does.
                collector.opponent_cache.insert(LoadedOpponentV1 {
                    policy: collector
                        .policy
                        .fork_for_collection_v3()?
                        .with_legacy_collection_sampler_v1(),
                    behavior: learner.clone(),
                });
            }
            let opponent = episode
                .opponent
                .as_ref()
                .map(|opponent| collector.opponent_cache.load(opponent))
                .transpose()?;
            let (trajectory, entries, ex1_record) = collect_episode_tolerant_v1(
                &mut collector.policy,
                &learner,
                opponent,
                episode,
                index,
                max_non_natural_episode_fraction,
                ex1.as_ref(),
            )
            .map_err(|(collection_error, entries)| {
                ledger
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .extend(entries);
                collection_error
            })?;
            ledger
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend(entries);
            // Publish directly from each private worker. The published bytes,
            // at most one batch of them, are kept with the trajectory they were
            // serialized from for validation and the update
            // (`trajectory_parse_ahead_v1`).
            let name = format!("episode-{index:04}.json");
            let bytes = serialize_json_v1(&name, &trajectory)?;
            let pin = publish_json_bytes_v1(&output_directory, &name, &bytes)?;
            trajectory_parse_ahead_v1::offer_serialized_v1(&pin.sha256, bytes, trajectory)?;
            let ex1_pin = ex1_record
                .map(|record| {
                    publish_json(&output_directory, &format!("ex1-{index:04}.json"), &record)
                })
                .transpose()?;
            Ok((pin, ex1_pin))
        },
        trajectory_parse_ahead_v1::start_helper_v1,
    );
    let mut ledger_entries = ledger
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    ledger_entries.sort_by_key(|entry| (entry.slot, entry.attempt));
    let failed_slots: BTreeSet<usize> = ledger_entries.iter().map(|entry| entry.slot).collect();
    let failed_fraction = failed_slots.len() as f32 / episodes.len() as f32;
    let non_natural_ledger_pin = if max_non_natural_episode_fraction > 0.0 {
        let document = NonNaturalLedgerV1 {
            schema: NON_NATURAL_LEDGER_SCHEMA_V1.into(),
            entries: ledger_entries,
        };
        Some(publish_json(
            &output_directory,
            "non-natural.json",
            &document,
        )?)
    } else {
        None
    };
    let (outputs, worker_timings) = collection_result?;
    let (outputs, ex1_outputs): (Vec<PinnedFileV1>, Vec<Option<PinnedFileV1>>) =
        outputs.into_iter().unzip();
    if max_non_natural_episode_fraction > 0.0 {
        ensure(
            failed_fraction <= max_non_natural_episode_fraction,
            &format!(
                "non-natural episode fraction {failed_fraction} exceeds the configured maximum \
                 {max_non_natural_episode_fraction}"
            ),
        )?;
    }
    let mut result = json!({
        "schema":"mtg-kernel-expanded-deck-collection/v1",
        "complete":true,
        "source":source,
        "behavior_state_sha256":state_hash,
        "trajectories":outputs,
        "collection_backend":"native-cpu-parallel-episodes-v1",
        "collection_workers_requested":workers,
        "collection_workers_started":worker_count,
        "collection_initialization_seconds":initialization_seconds,
        "collection_elapsed_seconds":started.elapsed().as_secs_f64(),
        "collection_worker_timings":worker_timings,
    });
    collection_sampler.record_v1(&mut result);
    training_math.record_v1(&mut result);
    if let Some(record) = &input_contract {
        result["input_contract"] = json!(record);
    }
    if let Some(pin) = non_natural_ledger_pin {
        result["non_natural_ledger"] = json!(pin);
    }
    if let Some(options) = &ex1 {
        let episodes: Vec<PinnedFileV1> = ex1_outputs.into_iter().flatten().collect();
        result["ex1"] = json!({"options":options,"episodes":episodes});
    }
    publish_json(&output_directory, "collection.json", &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn phase1_parallel_jobs_overlap_and_join_in_schedule_order() {
        let barrier = Arc::new(Barrier::new(4));
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let idle = AtomicUsize::new(0);
        let (results, timings) = ordered_parallel_jobs_v1(
            vec![0usize; 4],
            40,
            |seen, index| {
                *seen += 1;
                let concurrent = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(concurrent, Ordering::SeqCst);
                if *seen == 1 {
                    barrier.wait();
                }
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(index * 17)
            },
            || {
                idle.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        assert_eq!(results, (0..40).map(|i| i * 17).collect::<Vec<_>>());
        assert_eq!(idle.load(Ordering::SeqCst), 4);
        assert_eq!(peak.load(Ordering::SeqCst), 4);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(timings.len(), 4);
        assert_eq!(
            timings.iter().map(|t| t.completed_episodes).sum::<usize>(),
            40
        );
        assert!(timings.iter().all(|t| t.completed_episodes >= 1));
    }

    #[test]
    fn phase1_parallel_jobs_return_error_and_join_active_workers() {
        let active = AtomicUsize::new(0);
        let barrier = Barrier::new(2);
        let result = ordered_parallel_jobs_v1(
            vec![(); 2],
            20,
            |_, index| {
                active.fetch_add(1, Ordering::SeqCst);
                if index < 2 {
                    barrier.wait();
                }
                active.fetch_sub(1, Ordering::SeqCst);
                if index == 0 {
                    Err("deliberate failure".to_string())
                } else {
                    Ok(index)
                }
            },
            // The other worker may run out of jobs before the failure is
            // visible to it; idle work then starts and must not disturb the
            // reported error.
            || (),
        );
        assert!(result.unwrap_err().contains("deliberate failure"));
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn phase1_parallel_jobs_convert_panic_to_failure() {
        let result = ordered_parallel_jobs_v1(
            vec![(); 2],
            4,
            |_, index| {
                assert_ne!(index, 0, "deliberate panic");
                Ok(index)
            },
            || (),
        );
        assert!(result.unwrap_err().contains("worker panicked"));
    }

    #[test]
    fn phase1_parallel_jobs_reject_invalid_dimensions() {
        for (workers, jobs) in [(0, 1), (2, 1), (1, 0)] {
            assert!(
                ordered_parallel_jobs_v1(vec![(); workers], jobs, |_, i| Ok(i), || ()).is_err()
            );
        }
        assert!(validate_collection_workers_v1(0).is_err());
        assert!(validate_collection_workers_v1(1025).is_err());
        assert!(validate_collection_workers_v1(1).is_ok());
        assert!(validate_collection_workers_v1(1024).is_ok());
    }

    #[test]
    fn phase1_parallel_command_preserves_legacy_collection_shape() {
        let source = json!({
            "play_import":{"path":"unused-import.json", "sha256":"a".repeat(64)},
            "feature_transfer":{
                "expected_feature_contract_digest":FEATURE_CONTRACT_DIGEST_V3,
                "expected_feature_encoding_digest":FEATURE_ENCODING_DIGEST_V3
            },
            "checkpoint":null
        });
        let legacy = json!({"mode":"collect", "source":source, "episodes":[],
            "output_directory":"unused-collection"});
        let command: ExpandedTrainingCommandV1 = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(serde_json::to_value(command).unwrap(), legacy);
        let mut parallel = legacy.clone();
        parallel["mode"] = json!("collect_parallel");
        parallel["workers"] = json!(4);
        let command: ExpandedTrainingCommandV1 = serde_json::from_value(parallel.clone()).unwrap();
        assert_eq!(serde_json::to_value(command).unwrap(), parallel);
        let mut invalid = legacy;
        invalid["workers"] = json!(4);
        assert!(serde_json::from_value::<ExpandedTrainingCommandV1>(invalid).is_err());
        parallel.as_object_mut().unwrap().remove("workers");
        assert!(serde_json::from_value::<ExpandedTrainingCommandV1>(parallel).is_err());
    }

    /// A five-game parallel collection on two workers under a fresh temporary
    /// root, returning the root and the published trajectory pins.
    fn collect_parallel_for_parse_ahead_test_v1(label: &str) -> (PathBuf, Vec<PinnedFileV1>) {
        let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V3;
        let root = std::env::temp_dir().join(format!(
            "expanded-collect-parallel-{label}-{}",
            std::process::id()
        ));
        let source_struct =
            fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
                &root.join("source"),
                feature_identity,
                |_parameters| {},
            );
        let descriptor_path = root.join("descriptor.json");
        let descriptor_bytes = serde_json::to_vec(&source_struct).unwrap();
        fs::write(&descriptor_path, &descriptor_bytes).unwrap();
        let source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: descriptor_path.canonicalize().unwrap(),
                sha256: sha(&descriptor_bytes),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
                expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
            },
            checkpoint: None,
        };
        let deck = |id: &str| {
            let registration =
                crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
            let cards = registration.registered_configuration();
            ExpandedDeckListV1 {
                label: id.into(),
                mainboard: cards.mainboard().to_vec(),
                sideboard: cards.sideboard().to_vec(),
            }
        };
        let decks = [deck("Affinity"), deck("Terror")];
        let episodes: Vec<_> = (0..5u64)
            .map(|index| ExpandedEpisodeV1 {
                id: format!("{label}-{index}"),
                seed: 2_026_101_600 + index,
                starting_player: (index % 2) as u8,
                learner_seat: 0,
                opponent: None,
                opponent_kind: None,
                opponent_search: None,
                registered: decks.clone(),
                selected: decks.clone(),
                postboard: false,
                max_physical_decisions: 100_000,
                max_policy_steps: 1_000_000,
                runtime_rules: None,
                reset: None,
                opening: None,
                mulligan_modes: None,
            })
            .collect();
        let result = execute_v1(ExpandedTrainingCommandV1::CollectParallel {
            source,
            episodes,
            workers: 2,
            max_non_natural_episode_fraction: 0.0,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            output_directory: root.join("collect"),
        })
        .unwrap();
        let pins = serde_json::from_value(result["trajectories"].clone()).unwrap();
        (root, pins)
    }

    /// Parse-ahead end to end: every trajectory a parallel collection
    /// publishes is offered under its pin and parsed by idle workers, and
    /// validation's fields and the update's reader receive exactly a fresh
    /// parse of the file.
    #[test]
    fn parallel_collection_parses_its_trajectories_ahead_for_both_readers() {
        let (root, pins) = collect_parallel_for_parse_ahead_test_v1("parse-ahead");
        for pin in &pins {
            trajectory_parse_ahead_v1::wait_until_parsed_for_test_v1(&pin.sha256);
        }
        let read = read_trajectory_batch_v1(&pins, 2).unwrap();
        for (pin, trajectory) in pins.iter().zip(&read) {
            let bytes = fs::read(&pin.path).unwrap();
            let fresh: ExpandedTrajectoryV1 = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                serde_json::to_vec(trajectory).unwrap(),
                serde_json::to_vec(&fresh).unwrap()
            );
            let document: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                take_collection_fields_v1(&pin.sha256, &bytes),
                Some(CollectionTrajectoryFieldsV1::of_document_v1(&document))
            );
            assert_eq!(
                trajectory_parse_ahead_v1::served_for_test_v1(&pin.sha256),
                [true, true]
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    /// Stage 1's failure-order check for parse-ahead (collab actor-learner
    /// design v1, section 2.6): a trajectory changed after it was parsed
    /// ahead fails the update's reader with exactly the error the reader
    /// gives without parse-ahead, since the pin check comes first; the held
    /// parse is never handed to a reader of other bytes; and once the
    /// published bytes are back, it is served to them.
    #[test]
    fn a_trajectory_changed_after_its_parse_ahead_fails_its_readers_as_before() {
        let (root, pins) = collect_parallel_for_parse_ahead_test_v1("parse-ahead-changed");
        for pin in &pins {
            trajectory_parse_ahead_v1::wait_until_parsed_for_test_v1(&pin.sha256);
        }
        let original = fs::read(&pins[1].path).unwrap();
        let mut changed = original.clone();
        let middle = changed.len() / 2;
        changed[middle] ^= 1;
        fs::write(&pins[1].path, &changed).unwrap();
        // The pin check's error, which the two-worker reader reports with the
        // file's position, as it did before parse-ahead.
        let expected = format!(
            "preparation group 1: {}",
            read_pinned_bytes(&pins[1]).unwrap_err()
        );
        assert_eq!(read_trajectory_batch_v1(&pins, 2).unwrap_err(), expected);
        assert_eq!(take_collection_fields_v1(&pins[1].sha256, &changed), None);
        assert_eq!(
            trajectory_parse_ahead_v1::served_for_test_v1(&pins[1].sha256),
            [false, false]
        );
        fs::write(&pins[1].path, &original).unwrap();
        let read = read_trajectory_batch_v1(&pins, 2).unwrap();
        let fresh: ExpandedTrajectoryV1 = serde_json::from_slice(&original).unwrap();
        assert_eq!(
            serde_json::to_vec(&read[1]).unwrap(),
            serde_json::to_vec(&fresh).unwrap()
        );
        assert_eq!(
            trajectory_parse_ahead_v1::served_for_test_v1(&pins[1].sha256),
            [false, true]
        );
        fs::remove_dir_all(root).unwrap();
    }

    /// Mirrors `expanded_deck_training_v1::tests::
    /// execute_v1_collect_writes_ledger_records_its_hash_and_enforces_the_fraction_cap`
    /// through the parallel path specifically: there, ledger entries reach
    /// disk through a `Mutex`-guarded side channel (see `collect_parallel_v1`'s
    /// doc comment on `ledger`) instead of a plain `Vec` accumulated by a
    /// single thread, since workers run concurrently.
    #[test]
    fn phase1_parallel_tolerant_collection_ledgers_and_enforces_the_fraction_cap() {
        let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V3;
        let root = std::env::temp_dir().join(format!(
            "expanded-collect-parallel-tolerant-{}",
            std::process::id()
        ));
        let source_struct =
            fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
                &root.join("source"),
                feature_identity,
                |_parameters| {},
            );
        let descriptor_path = root.join("descriptor.json");
        let descriptor_bytes = serde_json::to_vec(&source_struct).unwrap();
        fs::write(&descriptor_path, &descriptor_bytes).unwrap();
        let source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: descriptor_path.canonicalize().unwrap(),
                sha256: sha(&descriptor_bytes),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
                expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
            },
            checkpoint: None,
        };
        let deck = |id: &str| {
            let registration =
                crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
            let cards = registration.registered_configuration();
            ExpandedDeckListV1 {
                label: id.into(),
                mainboard: cards.mainboard().to_vec(),
                sideboard: cards.sideboard().to_vec(),
            }
        };
        let decks = [deck("Affinity"), deck("Terror")];
        let episodes: Vec<_> = (0..4u64)
            .map(|index| ExpandedEpisodeV1 {
                id: format!("tolerant-parallel-{index}"),
                seed: 2_026_091_900 + index,
                starting_player: (index % 2) as u8,
                learner_seat: 0,
                opponent: None,
                opponent_kind: None,
                opponent_search: None,
                registered: decks.clone(),
                selected: decks.clone(),
                postboard: false,
                max_physical_decisions: 100_000,
                max_policy_steps: 1_000_000,
                runtime_rules: None,
                reset: None,
                opening: None,
                mulligan_modes: None,
            })
            .collect();

        // One slot fails its original attempt; a generous fraction budget
        // (1 of 4 = 0.25, budget 0.5) admits the retried result.
        set_force_non_natural_seeds_for_test_v1([episodes[1].seed]);
        let result = execute_v1(ExpandedTrainingCommandV1::CollectParallel {
            source: source.clone(),
            episodes: episodes.clone(),
            workers: 2,
            max_non_natural_episode_fraction: 0.5,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            output_directory: root.join("collect-admitted"),
        });
        clear_force_non_natural_seeds_for_test_v1([episodes[1].seed]);
        let result = result.unwrap();
        let trajectories = result["trajectories"].as_array().unwrap();
        assert_eq!(
            trajectories.len(),
            episodes.len(),
            "counts of actual completed work: exactly one trajectory per slot"
        );
        let ledger_pin: PinnedFileV1 =
            serde_json::from_value(result["non_natural_ledger"].clone()).unwrap();
        let ledger_document: Value = read_pinned(&ledger_pin).unwrap();
        let entries = ledger_document["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["slot"], 1);
        assert_eq!(entries[0]["seed"], episodes[1].seed);

        // Same setup, but a fraction budget too tight for even one failed
        // slot out of four must abort, with the ledger still preserved.
        let forced_seed = episodes[1].seed;
        set_force_non_natural_seeds_for_test_v1([forced_seed]);
        let error = execute_v1(ExpandedTrainingCommandV1::CollectParallel {
            source,
            episodes,
            workers: 2,
            max_non_natural_episode_fraction: 0.1,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            output_directory: root.join("collect-fraction-capped"),
        })
        .unwrap_err();
        clear_force_non_natural_seeds_for_test_v1([forced_seed]);
        assert!(
            error.contains("non-natural episode fraction"),
            "got: {error}"
        );
        assert!(
            root.join("collect-fraction-capped/non-natural.json")
                .exists(),
            "the ledger must be preserved on disk even though the fraction cap aborted the run"
        );
    }

    /// Real complete games, deliberately opt-in because these exercise native
    /// engine compute. This is a deterministic collection check, not strength.
    #[test]
    #[ignore = "root-owned native qualification: four Burn mirror games at workers 1, 2 and 4"]
    fn phase1_parallel_real_episodes_match_serial() {
        let template = FrozenPlayPolicyV1::training_fixture_v3();
        let source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: "fixture-only-import.json".into(),
                sha256: "a".repeat(64),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
                expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
            },
            checkpoint: None,
        };
        let mut model =
            NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
                .unwrap();
        model
            .replace_parameter_snapshot_v1(&template.training_parameters_v3())
            .unwrap();
        let state = NativePolicyValueTrainStateV1::new_v1(model).unwrap();
        let learner = ExpandedSeatBehaviorV1 {
            identity: inference_identity_v1(&source, &template, &state).unwrap(),
            source: source.clone(),
        };
        drop(state);
        let registered =
            crate::sideboard::checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
        let deck = ExpandedDeckListV1 {
            label: "Burn".into(),
            mainboard: registered.registered_configuration().mainboard().to_vec(),
            sideboard: registered.registered_configuration().sideboard().to_vec(),
        };
        let episodes: Vec<_> = (0..4)
            .map(|index| ExpandedEpisodeV1 {
                id: format!("phase1-parallel-fixture-{index}"),
                seed: 2_026_091_301 + index,
                starting_player: (index % 2) as u8,
                learner_seat: (index % 2) as u8,
                // Exercise common-model self-play and a separately owned
                // policy for the other physical seat with the same weights.
                opponent: (index >= 2).then(|| source.clone()),
                opponent_kind: None,
                opponent_search: None,
                registered: [deck.clone(), deck.clone()],
                selected: [deck.clone(), deck.clone()],
                postboard: false,
                max_physical_decisions: 100_000,
                max_policy_steps: 1_000_000,
                runtime_rules: None,
                reset: None,
                opening: None,
                mulligan_modes: None,
            })
            .collect();
        let context = || {
            (
                template.fork_for_collection_v3().unwrap(),
                LoadedOpponentV1 {
                    policy: template.fork_for_collection_v3().unwrap(),
                    behavior: learner.clone(),
                },
            )
        };
        let collect = |context: &mut (FrozenPlayPolicyV1, LoadedOpponentV1), index: usize| {
            let opponent = episodes[index].opponent.as_ref().map(|_| &mut context.1);
            let trajectory = collect_episode(&mut context.0, &learner, opponent, &episodes[index])?;
            serde_json::to_vec(&trajectory).map_err(err)
        };
        let mut serial_context = context();
        let serial: Vec<_> = (0..episodes.len())
            .map(|index| collect(&mut serial_context, index).unwrap())
            .collect();
        for workers in [2, 4] {
            let contexts = (0..workers).map(|_| context()).collect();
            let (parallel, _) =
                ordered_parallel_jobs_v1(contexts, episodes.len(), collect, || ()).unwrap();
            assert_eq!(parallel, serial, "worker count {workers}");
        }
    }

    /// Line (b) collection sampler end to end through the real commands:
    /// serial Collect and CollectParallel publish byte-identical trajectories.
    /// Learner rows carry the unclamped identity at every width; opponent
    /// rows keep the legacy samplers, including a current-self opponent (the
    /// `initial` assignment at the first update, forked from the learner in
    /// the parallel path). Tampered identities or selections fail
    /// validation, and the legacy default publishes no new key.
    #[test]
    fn unclamped_learner_collection_is_serial_parallel_identical_and_keeps_opponents_legacy() {
        let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V4;
        let root =
            std::env::temp_dir().join(format!("expanded-collect-unclamped-{}", std::process::id()));
        let source_struct =
            fresh_initialization_source::write_synthetic_fresh_source_with_parameters_v1(
                &root.join("source"),
                feature_identity,
                |_parameters| {},
            );
        let descriptor_path = root.join("descriptor.json");
        let descriptor_bytes = serde_json::to_vec(&source_struct).unwrap();
        fs::write(&descriptor_path, &descriptor_bytes).unwrap();
        let source = ExpandedModelSourceV1 {
            play_import: PinnedFileV1 {
                path: descriptor_path.canonicalize().unwrap(),
                sha256: sha(&descriptor_bytes),
            },
            feature_transfer: FrozenPlayObservationTransferV3 {
                expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
                expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
            },
            checkpoint: None,
        };
        let deck = |id: &str| {
            let registration =
                crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
            let cards = registration.registered_configuration();
            ExpandedDeckListV1 {
                label: id.into(),
                mainboard: cards.mainboard().to_vec(),
                sideboard: cards.sideboard().to_vec(),
            }
        };
        let decks = [deck("Burn"), deck("Faeries")];
        let episodes: Vec<_> = (0..4u64)
            .map(|index| ExpandedEpisodeV1 {
                id: format!("unclamped-{index}"),
                seed: 2_026_092_701 + index,
                starting_player: (index % 2) as u8,
                learner_seat: (index % 2) as u8,
                opponent: (index >= 2).then(|| source.clone()),
                opponent_search: None,
                opponent_kind: None,
                registered: decks.clone(),
                selected: decks.clone(),
                postboard: false,
                max_physical_decisions: 100_000,
                max_policy_steps: 1_000_000,
                runtime_rules: None,
                reset: None,
                opening: None,
                mulligan_modes: None,
            })
            .collect();
        let run = |label: &str, sampler: CollectionSamplerV1, workers: usize| {
            let output_directory = root.join(label);
            let command = if workers == 1 {
                ExpandedTrainingCommandV1::Collect {
                    source: source.clone(),
                    episodes: episodes.clone(),
                    max_non_natural_episode_fraction: 0.0,
                    collection_sampler: sampler,
                    training_math: TrainingMathV1::PlatformLibm,
                    ex1: None,
                    output_directory,
                }
            } else {
                ExpandedTrainingCommandV1::CollectParallel {
                    source: source.clone(),
                    episodes: episodes.clone(),
                    workers,
                    max_non_natural_episode_fraction: 0.0,
                    collection_sampler: sampler,
                    training_math: TrainingMathV1::PlatformLibm,
                    ex1: None,
                    output_directory,
                }
            };
            let result = execute_v1(command).unwrap();
            assert_eq!(
                result.get("collection_sampler").and_then(Value::as_str),
                sampler.identity_v1()
            );
            let pins: Vec<PinnedFileV1> =
                serde_json::from_value(result["trajectories"].clone()).unwrap();
            pins.iter()
                .map(|pin| read_pinned_bytes(pin).unwrap())
                .collect::<Vec<_>>()
        };
        let unclamped = CollectionSamplerV1::UnclampedSoftmaxF64IcdfU53V1;
        let serial = run("serial-unclamped", unclamped, 1);
        assert_eq!(run("parallel-unclamped", unclamped, 2), serial);
        let legacy_serial = run("serial-legacy", CollectionSamplerV1::Legacy, 1);
        assert_eq!(
            run("parallel-legacy", CollectionSamplerV1::Legacy, 2),
            legacy_serial
        );
        let (mut learner_rows, mut opponent_rows) = (0, 0);
        for (index, (bytes, legacy_bytes)) in serial.iter().zip(&legacy_serial).enumerate() {
            let legacy_text = String::from_utf8(legacy_bytes.clone()).unwrap();
            assert!(!legacy_text.contains("learner_sampler"));
            assert!(!legacy_text.contains(UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1));
            let trajectory: ExpandedTrajectoryV1 = serde_json::from_slice(bytes).unwrap();
            assert_eq!(
                trajectory.learner_sampler.as_deref(),
                Some(UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1)
            );
            let episode = &episodes[index];
            for row in &trajectory.decisions {
                if episode.opponent.is_none() || row.actor == episode.learner_seat {
                    learner_rows += 1;
                    assert_eq!(
                        row.sampler_identity.as_deref(),
                        Some(UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1)
                    );
                } else {
                    opponent_rows += 1;
                    assert_eq!(
                        row.sampler_identity.as_deref(),
                        decision_sampler_identity_v1(row.logits.len())
                    );
                }
            }
            let mut changed = trajectory.clone();
            changed.learner_sampler = None;
            assert!(validate_trajectory(&changed).is_err());
            let mut changed = trajectory.clone();
            changed.learner_sampler = Some("unknown-sampler".into());
            assert!(validate_trajectory(&changed).is_err());
            let learner_choice = trajectory
                .decisions
                .iter()
                .position(|row| {
                    row.logits.len() > 1
                        && (episode.opponent.is_none() || row.actor == episode.learner_seat)
                })
                .unwrap();
            let mut changed = trajectory.clone();
            changed.decisions[learner_choice].sampler_identity = None;
            assert_eq!(
                validate_trajectory(&changed).unwrap_err(),
                "stored decision sampler identity differs from action width"
            );
            let mut changed = trajectory.clone();
            let row = &mut changed.decisions[learner_choice];
            row.selected = (row.selected + 1) % row.logits.len() as u32;
            assert_eq!(
                validate_trajectory(&changed).unwrap_err(),
                "stored action differs from recorded behavior sampler"
            );
            // A hook cannot silently override the recorded learner sampler.
            let mut hook = |_: &DecisionRecordV1| Ok(SeatRowDrawV1::None);
            assert_eq!(
                validate_episode_records_with_search_v1(
                    episode.configurations().unwrap(),
                    episode,
                    &trajectory.configuration_sha256,
                    &trajectory.decisions,
                    &trajectory.terminal,
                    trajectory.learner_sampler.as_deref(),
                    Some((episode.learner_seat, &mut hook)),
                )
                .unwrap_err(),
                "hooked seat conflicts with learner collection sampler or is invalid"
            );
            if episode.opponent.is_some() {
                // Hooked logits stay legacy even with an unclamped learner.
                let mut hook = |row: &DecisionRecordV1| {
                    assert_eq!(
                        row.sampler_identity.as_deref(),
                        decision_sampler_identity_v1(row.logits.len())
                    );
                    Ok(SeatRowDrawV1::Logits)
                };
                validate_episode_records_with_search_v1(
                    episode.configurations().unwrap(),
                    episode,
                    &trajectory.configuration_sha256,
                    &trajectory.decisions,
                    &trajectory.terminal,
                    trajectory.learner_sampler.as_deref(),
                    Some((1 - episode.learner_seat, &mut hook)),
                )
                .unwrap();
            }
        }
        assert!(learner_rows > 0 && opponent_rows > 0);
    }

    #[test]
    fn collection_sampler_is_omitted_when_legacy_and_named_by_version_otherwise() {
        assert_eq!(
            serde_json::to_value(CollectionSamplerV1::UnclampedSoftmaxF64IcdfU53V1).unwrap(),
            json!(UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1)
        );
        let source = json!({
            "play_import":{"path":"unused-import.json", "sha256":"a".repeat(64)},
            "feature_transfer":{
                "expected_feature_contract_digest":FEATURE_CONTRACT_DIGEST_V3,
                "expected_feature_encoding_digest":FEATURE_ENCODING_DIGEST_V3
            },
            "checkpoint":null
        });
        for mode in ["collect", "collect_parallel"] {
            let mut legacy = json!({"mode":mode, "source":source, "episodes":[],
                "output_directory":"unused-collection"});
            if mode == "collect_parallel" {
                legacy["workers"] = json!(2);
            }
            let command: ExpandedTrainingCommandV1 =
                serde_json::from_value(legacy.clone()).unwrap();
            assert_eq!(serde_json::to_value(command).unwrap(), legacy);
            let mut unclamped = legacy.clone();
            unclamped["collection_sampler"] = json!(UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1);
            let command: ExpandedTrainingCommandV1 =
                serde_json::from_value(unclamped.clone()).unwrap();
            assert_eq!(serde_json::to_value(command).unwrap(), unclamped);
            let mut unknown = legacy;
            unknown["collection_sampler"] = json!("f32-q8-expq63-hamilton-splitmix64-v1");
            assert!(serde_json::from_value::<ExpandedTrainingCommandV1>(unknown).is_err());
        }
    }
}
