//! Execution-only preparation of one fixed update. Independent physical groups
//! replay against immutable models; loss, backward and Adam stay in the caller's
//! original order. No model/RNG state is mutated by these workers.

use super::*;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Instant;

const MAX_WORKERS: usize = 32;
const MAX_OPPONENTS: usize = 32;
const MAX_GROUPS: usize = 65_536;
/// Bound on the decoded tensor payload one prepared update may hold. The
/// historical constant (256 MiB) is the default; since campaign 002 it is
/// config-driven through `max_prepared_tensor_mebibytes` on the run config
/// and the prepared update command, so an update can carry more games than
/// ten without silently changing any existing config (absent means 256).
/// Admitted range 64..=4096 MiB; the actual bound is recorded in the
/// preparation telemetry (`max_decoded_tensor_bytes`) and re-verified on
/// resume against the run configuration.
pub(crate) const DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES: usize = 256;
pub(crate) const MIN_PREPARED_TENSOR_MEBIBYTES: usize = 64;
pub(crate) const MAX_PREPARED_TENSOR_MEBIBYTES: usize = 4096;
const MEBIBYTE: usize = 1024 * 1024;

pub(crate) fn validate_max_prepared_tensor_mebibytes_v1(mebibytes: usize) -> Result<(), String> {
    ensure(
        (MIN_PREPARED_TENSOR_MEBIBYTES..=MAX_PREPARED_TENSOR_MEBIBYTES).contains(&mebibytes),
        "max_prepared_tensor_mebibytes must be in 64..=4096",
    )
}
const WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn validate_preparation_workers_v1(workers: usize) -> Result<(), String> {
    ensure(
        (1..=MAX_WORKERS).contains(&workers),
        "preparation workers must be in 1..=32",
    )
}

#[derive(Debug, Serialize)]
pub(super) struct WorkerTimingV1 {
    worker: usize,
    completed_groups: usize,
    busy_wall_seconds: f64,
}

#[derive(Debug, Serialize)]
pub(super) struct PreparationTelemetryV1 {
    schema: &'static str,
    requested_workers: usize,
    started_workers: usize,
    physical_group_jobs: usize,
    distinct_opponent_sources: usize,
    opponent_load_calls: usize,
    opponent_cache_hits: usize,
    current_model_reuses: usize,
    decoded_tensor_payload_bytes: usize,
    max_decoded_tensor_bytes: usize,
    worker_stack_reservation_bytes: usize,
    validation_and_planning_seconds: f64,
    opponent_loading_seconds: f64,
    behavior_binding_seconds: f64,
    parallel_replay_seconds: f64,
    preparation_elapsed_seconds: f64,
    worker_timings: Vec<WorkerTimingV1>,
    timing_scope: &'static str,
    memory_scope: &'static str,
}

pub(super) struct PreparedGroupsV1<'a> {
    pub(super) groups: Vec<LearnerTensorGroupV1<'a>>,
    pub(super) telemetry: PreparationTelemetryV1,
}

#[derive(Clone, Copy)]
struct GroupJobV1 {
    episode: usize,
    start: usize,
    end: usize,
    opponent: Option<usize>,
}

fn decoded_payload_bytes(t: &TensorBitsV1) -> Result<usize, String> {
    let mut bytes = 0usize;
    for values in [
        &t.state,
        &t.object_features,
        &t.edge_features,
        &t.action_features,
        &t.action_ref_features,
    ] {
        bytes = bytes
            .checked_add(std::mem::size_of_val(values.as_slice()))
            .ok_or("decoded tensor payload overflow")?;
    }
    for values in [
        &t.object_card_ids,
        &t.object_groups,
        &t.object_node_ids,
        &t.edge_source_indices,
        &t.edge_target_indices,
        &t.action_ref_card_ids,
        &t.action_ref_action_indices,
        &t.action_ref_node_indices,
    ] {
        bytes = bytes
            .checked_add(std::mem::size_of_val(values.as_slice()))
            .ok_or("decoded tensor payload overflow")?;
    }
    Ok(bytes)
}

fn plan_v1(
    episodes: &[ExpandedTrajectoryV1],
    max_decoded_tensor_bytes: usize,
) -> Result<(Vec<GroupJobV1>, Vec<ExpandedModelSourceV1>, usize), String> {
    ensure(
        !episodes.is_empty() && episodes.len() <= 1024,
        "invalid preparation episode count",
    )?;
    let mut sources = Vec::new();
    let mut jobs = Vec::new();
    let mut payload = 0usize;
    // Sequential per-trajectory validation retains the original seat RNG and
    // grouping checks. Only the independent forwards below become parallel.
    for (episode_index, episode) in episodes.iter().enumerate() {
        validate_trajectory(episode)?;
        let opponent = if let Some(source) = &episode.episode.opponent {
            let index = match sources.iter().position(|existing| existing == source) {
                Some(index) => index,
                None => {
                    ensure(
                        sources.len() < MAX_OPPONENTS,
                        "preparation exceeds 32 distinct opponents",
                    )?;
                    sources.push(source.clone());
                    sources.len() - 1
                }
            };
            Some(index)
        } else {
            None
        };
        let mut start = 0;
        while start < episode.decisions.len() {
            let end = start + episode.decisions[start].substep_count as usize;
            for row in &episode.decisions[start..end] {
                payload = payload
                    .checked_add(decoded_payload_bytes(&row.tensor)?)
                    .ok_or("preparation tensor payload overflow")?;
                if payload > max_decoded_tensor_bytes {
                    return Err(format!(
                        "preparation exceeds {} MiB decoded tensor payload",
                        max_decoded_tensor_bytes / MEBIBYTE
                    ));
                }
            }
            ensure(
                jobs.len() < MAX_GROUPS,
                "preparation exceeds 65536 physical groups",
            )?;
            jobs.push(GroupJobV1 {
                episode: episode_index,
                start,
                end,
                opponent,
            });
            start = end;
        }
    }
    Ok((jobs, sources, payload))
}

/// Each result occupies its original ordinal. After a failure, lower in-flight
/// jobs still finish; monotonic dispatch ensures no lower job was skipped.
/// Higher jobs already in flight may finish, but never affect error selection.
fn ordered_jobs_v1<T: Send, F: Fn(usize) -> Result<T, String> + Sync>(
    workers: usize,
    job_count: usize,
    work: F,
) -> Result<(Vec<T>, Vec<WorkerTimingV1>), String> {
    validate_preparation_workers_v1(workers)?;
    ensure(
        job_count > 0 && job_count <= MAX_GROUPS,
        "invalid preparation job count",
    )?;
    let next = AtomicUsize::new(0);
    let first_error = AtomicUsize::new(job_count);
    thread::scope(|scope| {
        let mut handles = Vec::new();
        let mut spawn_error = None;
        for worker in 0..workers.min(job_count) {
            let next = &next;
            let first_error = &first_error;
            let work = &work;
            match thread::Builder::new()
                .name(format!("expanded-preparation-{worker}"))
                .stack_size(WORKER_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    let mut values = Vec::new();
                    let mut busy = 0.0;
                    loop {
                        let ordinal = next.fetch_add(1, Ordering::Relaxed);
                        if ordinal >= job_count || ordinal >= first_error.load(Ordering::Acquire) {
                            break;
                        }
                        let started = Instant::now();
                        let result = catch_unwind(AssertUnwindSafe(|| work(ordinal)))
                            .unwrap_or_else(|_| Err("preparation worker panicked".into()));
                        busy += started.elapsed().as_secs_f64();
                        if result.is_err() {
                            first_error.fetch_min(ordinal, Ordering::AcqRel);
                        }
                        values.push((ordinal, result));
                    }
                    let timing = WorkerTimingV1 {
                        worker,
                        completed_groups: values.len(),
                        busy_wall_seconds: busy,
                    };
                    (values, timing)
                }) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    first_error.store(0, Ordering::Release);
                    spawn_error = Some(format!(
                        "could not start preparation worker {worker}: {error}"
                    ));
                    break;
                }
            }
        }
        let mut ordered: Vec<Option<Result<T, String>>> = (0..job_count).map(|_| None).collect();
        let mut timings = Vec::new();
        let mut join_error = None;
        for handle in handles {
            match handle.join() {
                Ok((values, timing)) => {
                    timings.push(timing);
                    for (ordinal, result) in values {
                        if ordered[ordinal].replace(result).is_some() {
                            join_error = Some("duplicate preparation result ordinal".to_owned());
                        }
                    }
                }
                Err(_) => {
                    first_error.store(0, Ordering::Release);
                    join_error = Some("preparation worker panicked outside a job".to_owned());
                }
            }
        }
        if let Some(error) = spawn_error.or(join_error) {
            return Err(error);
        }
        let mut results = Vec::with_capacity(job_count);
        for (ordinal, result) in ordered.into_iter().enumerate() {
            match result {
                Some(Ok(value)) => results.push(value),
                Some(Err(error)) => return Err(format!("preparation group {ordinal}: {error}")),
                None => return Err(format!("missing preparation result {ordinal}")),
            }
        }
        Ok((results, timings))
    })
}

/// Process-wide cache of loaded opponent models keyed by their pinned source.
/// A run's fixed opponents (the block's initial policy and the frozen partner)
/// were parsed from their 20 MB checkpoints twice per update; now each source
/// is loaded once per process and every update receives a private fork
/// (`fork_for_collection_v3`, the path the current model already takes).
/// Replay still checks every row against the rollout's recorded logits, so a
/// fork that scored differently would fail the update rather than pass.
/// Bounded and least-recently-used; entries are immutable once inserted. A
/// policy that cannot be forked is used directly and never cached, as before.
const OPPONENT_CACHE_ENTRIES: usize = 8;

struct CachedOpponentV1 {
    source: ExpandedModelSourceV1,
    policy: Arc<FrozenPlayPolicyV1>,
    identity: ExpandedInferenceIdentityV1,
}

static OPPONENT_CACHE: OnceLock<Mutex<VecDeque<CachedOpponentV1>>> = OnceLock::new();

/// Load one opponent, forking it from the cache when its source is cached;
/// the flag reports a cache hit.
fn load_opponent_cached_v1(
    source: &ExpandedModelSourceV1,
) -> Result<(LoadedOpponentV1, bool), String> {
    let cache = OPPONENT_CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    let cached = {
        let mut entries = cache
            .lock()
            .map_err(|_| "opponent cache poisoned".to_string())?;
        match entries.iter().position(|entry| &entry.source == source) {
            Some(index) => {
                let entry = entries.remove(index).ok_or("opponent cache index")?;
                let policy = Arc::clone(&entry.policy);
                let identity = entry.identity.clone();
                entries.push_back(entry);
                Some((policy, identity))
            }
            None => None,
        }
    };
    if let Some((policy, identity)) = cached {
        return Ok((
            LoadedOpponentV1 {
                policy: policy.fork_for_collection_v3()?,
                behavior: ExpandedSeatBehaviorV1 {
                    source: source.clone(),
                    identity,
                },
            },
            true,
        ));
    }
    let (policy, identity) = load_expanded_inference_v1(source)?;
    let behavior = ExpandedSeatBehaviorV1 {
        source: source.clone(),
        identity: identity.clone(),
    };
    match policy.fork_for_collection_v3() {
        Ok(fork) => {
            let mut entries = cache
                .lock()
                .map_err(|_| "opponent cache poisoned".to_string())?;
            while entries.len() >= OPPONENT_CACHE_ENTRIES {
                entries.pop_front();
            }
            entries.push_back(CachedOpponentV1 {
                source: source.clone(),
                policy: Arc::new(policy),
                identity,
            });
            Ok((
                LoadedOpponentV1 {
                    policy: fork,
                    behavior,
                },
                false,
            ))
        }
        Err(_) => Ok((LoadedOpponentV1 { policy, behavior }, false)),
    }
}

pub(super) fn prepare_v1<'a>(
    episodes: &'a [ExpandedTrajectoryV1],
    policy: &FrozenPlayPolicyV1,
    learner: &ExpandedSeatBehaviorV1,
    workers: usize,
    max_prepared_tensor_mebibytes: usize,
) -> Result<PreparedGroupsV1<'a>, String> {
    let mut hits = 0;
    let mut prepared = prepare_with_loader_v1(
        episodes,
        policy,
        learner,
        workers,
        max_prepared_tensor_mebibytes,
        |source| {
            let (loaded, hit) = load_opponent_cached_v1(source)?;
            hits += usize::from(hit);
            Ok(loaded)
        },
    )?;
    prepared.telemetry.opponent_cache_hits = hits;
    Ok(prepared)
}

fn prepare_with_loader_v1<'a, F>(
    episodes: &'a [ExpandedTrajectoryV1],
    policy: &FrozenPlayPolicyV1,
    learner: &ExpandedSeatBehaviorV1,
    workers: usize,
    max_prepared_tensor_mebibytes: usize,
    mut load: F,
) -> Result<PreparedGroupsV1<'a>, String>
where
    F: FnMut(&ExpandedModelSourceV1) -> Result<LoadedOpponentV1, String>,
{
    validate_preparation_workers_v1(workers)?;
    validate_max_prepared_tensor_mebibytes_v1(max_prepared_tensor_mebibytes)?;
    let max_decoded_tensor_bytes = max_prepared_tensor_mebibytes * MEBIBYTE;
    let started = Instant::now();
    let (jobs, sources, payload) = plan_v1(episodes, max_decoded_tensor_bytes)?;
    let planning = started.elapsed().as_secs_f64();
    let loading_started = Instant::now();
    let mut loads = 0;
    let mut current_reuses = 0;
    // Error values remain attached to the source rather than returning before
    // an earlier trajectory's replay error has been examined.
    let opponents: Vec<Result<LoadedOpponentV1, String>> = sources
        .iter()
        .map(|source| {
            if source == &learner.source {
                current_reuses += 1;
                policy
                    .fork_for_collection_v3()
                    .map(|policy| LoadedOpponentV1 {
                        policy,
                        behavior: learner.clone(),
                    })
            } else {
                loads += 1;
                load(source)
            }
        })
        .collect();
    let loading = loading_started.elapsed().as_secs_f64();
    let binding_started = Instant::now();
    let bindings: Vec<Result<(), String>> = episodes
        .iter()
        .map(|episode| {
            let other = if let Some(source) = &episode.episode.opponent {
                let index = sources
                    .iter()
                    .position(|candidate| candidate == source)
                    .ok_or("preparation opponent index absent")?;
                Some(opponents[index].as_ref().map_err(Clone::clone)?)
            } else {
                None
            };
            validate_actual_behaviors_v1(episode, learner, other.map(|other| &other.behavior))
        })
        .collect();
    let binding = binding_started.elapsed().as_secs_f64();
    let replay_started = Instant::now();
    let (ordered, worker_timings) = ordered_jobs_v1(workers, jobs.len(), |ordinal| {
        let job = jobs[ordinal];
        bindings[job.episode].as_ref().map_err(Clone::clone)?;
        let trajectory = &episodes[job.episode];
        let first = &trajectory.decisions[job.start];
        let opponent = job
            .opponent
            .map(|index| opponents[index].as_ref().map_err(Clone::clone))
            .transpose()?;
        let acting = if first.actor == trajectory.episode.learner_seat {
            policy
        } else {
            opponent.map_or(policy, |other| &other.policy)
        };
        // The acting policy's own generation, not the trajectory's single
        // learner-derived digest pair: for an ordinary homogeneous
        // trajectory the two always agree (see `collect_episode`), but a V3
        // registry-transfer opponent's own rows under a V4-learner
        // trajectory must replay through its own V3 encoder even though the
        // trajectory-wide digest is V4. See
        // `expanded_deck_training_v1::replay_learner_groups_v1`, the serial
        // sibling of this same dispatch.
        let generation = acting.feature_identity_v1().generation;
        let mut rows = Vec::new();
        for row in &trajectory.decisions[job.start..job.end] {
            let common = row.tensor.tensor();
            let output = match generation {
                FreshLineageGenerationV1::V3 => acting.score_training_tensor_v3(
                    &NativeFlatDecisionTensorV3 {
                        common: common.clone(),
                    },
                )?,
                FreshLineageGenerationV1::V4 => acting.score_training_tensor_v4(
                    &NativeFlatDecisionTensorV4 {
                        common: common.clone(),
                    },
                )?,
            };
            ensure(
                bits(&output.logits) == row.logits && output.value.to_bits() == row.value,
                "stored tensor does not reproduce rollout outputs",
            )?;
            if first.actor == trajectory.episode.learner_seat {
                rows.push((row, common));
            }
        }
        Ok(if rows.is_empty() {
            None
        } else {
            Some((
                trajectory.terminal.terminal_reward[first.actor as usize] as i8,
                rows,
            ))
        })
    })?;
    let replay = replay_started.elapsed().as_secs_f64();
    drop(opponents);
    let groups = ordered.into_iter().flatten().collect();
    Ok(PreparedGroupsV1 {
        groups,
        telemetry: PreparationTelemetryV1 {
            schema: "mtg-kernel-ordered-update-preparation/v1",
            requested_workers: workers,
            started_workers: workers.min(jobs.len()),
            physical_group_jobs: jobs.len(),
            distinct_opponent_sources: sources.len(),
            opponent_load_calls: loads,
            opponent_cache_hits: 0,
            current_model_reuses: current_reuses,
            decoded_tensor_payload_bytes: payload,
            max_decoded_tensor_bytes,
            worker_stack_reservation_bytes: workers.min(jobs.len()) * WORKER_STACK_BYTES,
            validation_and_planning_seconds: planning,
            opponent_loading_seconds: loading,
            behavior_binding_seconds: binding,
            parallel_replay_seconds: replay,
            preparation_elapsed_seconds: started.elapsed().as_secs_f64(),
            worker_timings,
            timing_scope: "wall time, not CPU time or utilization; immutable model loads and replay only; backward/Adam unchanged",
            memory_scope: "decoded numeric tensor payload bound; excludes parsed input, model storage, vector headers, forward scratch and later learner tapes; at most 32 retained opponent policies and 32 worker stacks",
        },
    })
}

#[cfg(test)]
mod tests;
