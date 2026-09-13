//! Parallelize only independent episodes from one fixed collecting checkpoint.
//! Each worker owns its model, inference scratch, opponent cache and game/RNG
//! state. Only work indices are shared. Published trajectories and the learner
//! update keep the original schedule order, regardless of completion order.

use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

pub(crate) fn validate_collection_workers_v1(workers: usize) -> Result<(), String> {
    ensure(
        (1..=1024).contains(&workers),
        "collection workers must be in 1..=1024",
    )
}

#[derive(Debug, Serialize)]
struct WorkerTimingV1 {
    worker: usize,
    completed_episodes: usize,
    busy_seconds: f64,
}

struct CollectorV1 {
    policy: FrozenPlayPolicyV1,
    opponent_cache: OpponentCacheV1,
}

/// Bounded dynamic scheduling with no shared model lock and no worker left
/// alive on return. A failed job or panic cancels new dispatch; already active
/// jobs join before the caller can attempt an update or publish completion.
fn ordered_parallel_jobs_v1<W, T, F>(
    contexts: Vec<W>,
    job_count: usize,
    work: F,
) -> Result<(Vec<T>, Vec<WorkerTimingV1>), String>
where
    W: Send,
    T: Send,
    F: Fn(&mut W, usize) -> Result<T, String> + Sync,
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
            let spawned = thread::Builder::new()
                .name(format!("expanded-collector-{worker}"))
                .stack_size(8 * 1024 * 1024)
                .spawn_scoped(scope, move || {
                    let mut results = Vec::new();
                    let mut busy_seconds = 0.0;
                    while !cancelled.load(Ordering::Acquire) {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= job_count || cancelled.load(Ordering::Acquire) {
                            break;
                        }
                        let started = Instant::now();
                        let result = catch_unwind(AssertUnwindSafe(|| work(&mut context, index)));
                        busy_seconds += started.elapsed().as_secs_f64();
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
        // Join every child even after an error. There is no background work
        // that could race the scheduler's next collection or update phase.
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

pub(super) fn collect_parallel_v1(
    source: ExpandedModelSourceV1,
    episodes: Vec<ExpandedEpisodeV1>,
    workers: usize,
    output_directory: PathBuf,
) -> Result<Value, String> {
    let started = Instant::now();
    validate_collection_workers_v1(workers)?;
    ensure(workers >= 2, "use collect for a single collection worker")?;
    ensure(
        !episodes.is_empty() && episodes.len() <= 1024,
        "invalid collection size",
    )?;
    let mut ids = BTreeSet::new();
    for episode in &episodes {
        episode.configurations()?;
        ensure(ids.insert(&episode.id), "duplicate episode id")?;
    }
    ensure(
        !output_directory.exists(),
        "output directory already exists",
    )?;
    let (policy, state, transfer) = initialize_with_transfer_context(&source)?;
    if let Some(context) = &transfer {
        context.validate_batch(&episodes)?;
    }
    let state_hash = hex(&state.state_sha256_v1().map_err(err)?);
    let learner = ExpandedSeatBehaviorV1 {
        source: source.clone(),
        identity: inference_identity_v1(&source, &policy, &state)?,
    };
    drop(state);
    let worker_count = workers.min(episodes.len());
    let contexts = (0..worker_count)
        .map(|_| {
            Ok(CollectorV1 {
                policy: policy.fork_for_collection_v3()?,
                opponent_cache: OpponentCacheV1::default(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    drop(policy);
    let initialization_seconds = started.elapsed().as_secs_f64();
    fs::create_dir(&output_directory).map_err(err)?;
    let (outputs, worker_timings) =
        ordered_parallel_jobs_v1(contexts, episodes.len(), |collector, index| {
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
                && !collector
                    .opponent_cache
                    .entry
                    .as_ref()
                    .is_some_and(|entry| entry.behavior.source == source)
            {
                collector.opponent_cache.entry = Some(LoadedOpponentV1 {
                    policy: collector.policy.fork_for_collection_v3()?,
                    behavior: learner.clone(),
                });
            }
            let opponent = episode
                .opponent
                .as_ref()
                .map(|opponent| collector.opponent_cache.load(opponent))
                .transpose()?;
            let trajectory = collect_episode(&mut collector.policy, &learner, opponent, episode)?;
            // Publish directly from each private worker so completed tensors
            // do not accumulate in RAM while an earlier episode is running.
            publish_json(
                &output_directory,
                &format!("episode-{index:04}.json"),
                &trajectory,
            )
        })?;
    let result = json!({
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
        let (results, timings) = ordered_parallel_jobs_v1(vec![0usize; 4], 40, |seen, index| {
            *seen += 1;
            let concurrent = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(concurrent, Ordering::SeqCst);
            if *seen == 1 {
                barrier.wait();
            }
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(index * 17)
        })
        .unwrap();
        assert_eq!(results, (0..40).map(|i| i * 17).collect::<Vec<_>>());
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
        let result = ordered_parallel_jobs_v1(vec![(); 2], 20, |_, index| {
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
        });
        assert!(result.unwrap_err().contains("deliberate failure"));
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn phase1_parallel_jobs_convert_panic_to_failure() {
        let result = ordered_parallel_jobs_v1(vec![(); 2], 4, |_, index| {
            assert_ne!(index, 0, "deliberate panic");
            Ok(index)
        });
        assert!(result.unwrap_err().contains("worker panicked"));
    }

    #[test]
    fn phase1_parallel_jobs_reject_invalid_dimensions() {
        for (workers, jobs) in [(0, 1), (2, 1), (1, 0)] {
            assert!(ordered_parallel_jobs_v1(vec![(); workers], jobs, |_, i| Ok(i)).is_err());
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
                registered: [deck.clone(), deck.clone()],
                selected: [deck.clone(), deck.clone()],
                postboard: false,
                max_physical_decisions: 100_000,
                max_policy_steps: 1_000_000,
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
                ordered_parallel_jobs_v1(contexts, episodes.len(), collect).unwrap();
            assert_eq!(parallel, serial, "worker count {workers}");
        }
    }
}
