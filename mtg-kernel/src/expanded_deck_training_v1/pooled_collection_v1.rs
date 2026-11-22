//! Pooled collection (pooled-lineage stage 1): one update's episodes are
//! played by several collector processes, on one host or many, and merged
//! into the collection one `CollectParallel` process would have published.
//!
//! Why it is exact: a trajectory depends only on its episode, the collecting
//! source and the build. Every collector worker owns its model, opponent cache
//! and game RNG (`phase1_parallel_collection`), and a non-natural retry seed
//! derives from the episode alone (`derived_retry_seed_v1`). Which process
//! plays a slot therefore changes no byte of its trajectory, as the worker
//! count already does not.
//!
//! A shard command is the whole `CollectParallel` command with a contiguous
//! slice of its episodes and a [`CollectionShardV1`] that names the slice's
//! first schedule index. A shard publishes `episode-NNNN.json` under schedule
//! indices and ledgers non-natural attempts under schedule slots; it never
//! applies the non-natural fraction cap, which is a property of the whole
//! schedule. The merge checks every shard against the whole command,
//! re-hashes every trajectory, applies the cap to the whole schedule and
//! publishes the receipt fields `collect_parallel_v1` publishes, with
//! `pooled_shards` provenance. The run loop's own collection validation and
//! the update's replay check then read the merged collection exactly as they
//! read a local one.

use super::*;

/// The schedule position of a shard of one collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionShardV1 {
    /// Schedule index of the shard's first episode in the whole collection.
    pub first_slot: usize,
    /// Episode count of the whole collection.
    pub schedule_episodes: usize,
}

impl CollectionShardV1 {
    /// Checks a shard of `episodes` episodes against its schedule.
    pub(crate) fn validate_v1(&self, episodes: usize) -> Result<(), String> {
        ensure(
            (1..=1024).contains(&self.schedule_episodes)
                && episodes >= 1
                && self
                    .first_slot
                    .checked_add(episodes)
                    .is_some_and(|end| end <= self.schedule_episodes),
            "collection shard lies outside its schedule",
        )
    }
}

/// Execution of one shard: its episode count, collector workers and the
/// output directory on the host that plays it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShardSpecV1 {
    pub episodes: usize,
    pub workers: usize,
    pub output_directory: PathBuf,
}

/// A shard plan: one entry per shard, in schedule order.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShardPlanV1 {
    pub shards: Vec<ShardSpecV1>,
}

/// Receipt keys that describe one collector process, not the collection.
/// Every other key of a shard's receipt must be equal across shards and is
/// carried into the merged receipt.
const PER_SHARD_KEYS_V1: &[&str] = &[
    "schema",
    "complete",
    "source",
    "behavior_state_sha256",
    "trajectories",
    "collection_backend",
    "collection_workers_requested",
    "collection_workers_started",
    "collection_initialization_seconds",
    "collection_elapsed_seconds",
    "collection_worker_timings",
    "non_natural_ledger",
    "shard",
];

pub const POOLED_COLLECTION_BACKEND_V1: &str = "pooled-collection-shards-v1";

struct WholeCommandV1<'a> {
    source: &'a ExpandedModelSourceV1,
    episodes: &'a [ExpandedEpisodeV1],
    workers: usize,
    max_non_natural_episode_fraction: f32,
    collection_sampler: CollectionSamplerV1,
    training_math: TrainingMathV1,
    record_envelope: RecordEnvelopeV1,
    output_directory: &'a Path,
}

fn whole_command_v1(command: &ExpandedTrainingCommandV1) -> Result<WholeCommandV1<'_>, String> {
    match command {
        ExpandedTrainingCommandV1::CollectParallel {
            source,
            episodes,
            workers,
            max_non_natural_episode_fraction,
            collection_sampler,
            training_math,
            ex1,
            record_envelope,
            shard,
            output_directory,
            ..
        } => {
            ensure(
                shard.is_none(),
                "a pooled collection starts from a whole command",
            )?;
            ensure(ex1.is_none(), "EX1 collection is not available pooled")?;
            ensure(
                !episodes.is_empty() && episodes.len() <= 1024,
                "invalid collection size",
            )?;
            validate_max_non_natural_episode_fraction_v1(*max_non_natural_episode_fraction)?;
            // The ordinary collector's preflight for the whole command, before any
            // split: shards would each pass it alone.
            super::phase1_parallel_collection::validate_collection_workers_v1(*workers)?;
            ensure(*workers >= 2, "use collect for a single collection worker")?;
            let mut ids = BTreeSet::new();
            for episode in episodes.iter() {
                ensure(ids.insert(&episode.id), "duplicate episode id")?;
            }
            Ok(WholeCommandV1 {
                source,
                episodes,
                workers: *workers,
                max_non_natural_episode_fraction: *max_non_natural_episode_fraction,
                collection_sampler: *collection_sampler,
                training_math: *training_math,
                record_envelope: *record_envelope,
                output_directory,
            })
        }
        _ => Err("a pooled collection needs a collect_parallel command".into()),
    }
}

/// Splits a whole `CollectParallel` command into one command per shard, in
/// schedule order. Shards are contiguous and cover every episode once.
pub fn shard_commands_v1(
    command: &ExpandedTrainingCommandV1,
    shards: &[ShardSpecV1],
) -> Result<Vec<ExpandedTrainingCommandV1>, String> {
    let whole = whole_command_v1(command)?;
    ensure(!shards.is_empty(), "a pooled collection needs a shard")?;
    ensure(
        shards.iter().map(|shard| shard.episodes).sum::<usize>() == whole.episodes.len(),
        "shards must cover every episode exactly once",
    )?;
    let mut first_slot = 0;
    let mut commands = Vec::with_capacity(shards.len());
    for spec in shards {
        ensure(spec.episodes >= 1, "a shard needs an episode")?;
        // CollectParallel takes at least two requested workers; a shard of one
        // episode starts one.
        ensure(
            spec.workers >= 2,
            "a shard needs at least two requested workers",
        )?;
        let mut shard_command = command.clone();
        if let ExpandedTrainingCommandV1::CollectParallel {
            episodes,
            workers,
            shard,
            output_directory,
            ..
        } = &mut shard_command
        {
            *episodes = whole.episodes[first_slot..first_slot + spec.episodes].to_vec();
            *workers = spec.workers;
            *shard = Some(CollectionShardV1 {
                first_slot,
                schedule_episodes: whole.episodes.len(),
            });
            *output_directory = spec.output_directory.clone();
        }
        commands.push(shard_command);
        first_slot += spec.episodes;
    }
    Ok(commands)
}

fn read_json_file_v1(path: &Path) -> Result<(Vec<u8>, Value), String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    ensure(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "shard receipt exceeds file size limit",
    )?;
    let document = serde_json::from_slice(&bytes).map_err(err)?;
    Ok((bytes, document))
}

/// Reads a file a shard published under `name` in `directory` and checks it
/// against the shard's pin. Shard directories move between hosts, so only the
/// pin's file name and hash bind; the recorded path is the collector's.
fn read_shard_file_v1(directory: &Path, pin: &PinnedFileV1, name: &str) -> Result<Vec<u8>, String> {
    ensure(
        pin.path.file_name().and_then(|n| n.to_str()) == Some(name),
        "shard file is not under its schedule name",
    )?;
    let path = directory.join(name);
    let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    ensure(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "shard file exceeds file size limit",
    )?;
    ensure(
        sha(&bytes) == pin.sha256,
        "shard file SHA differs from its pin",
    )?;
    Ok(bytes)
}

/// Threads the merge reads, checks and publishes trajectories on.
fn merge_workers_v1(items: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .clamp(1, 16)
        .min(items.max(1))
}

/// `f(0..count)` on `merge_workers_v1(count)` threads, results in index order
/// (a panicking call yields `panicked`).
fn ordered_v1<T: Send>(
    count: usize,
    panicked: &str,
    f: impl Fn(usize) -> Result<T, String> + Sync,
) -> Vec<Result<T, String>> {
    let workers = merge_workers_v1(count);
    let next = std::sync::atomic::AtomicUsize::new(0);
    type DoneV1<T> = Vec<(usize, Result<T, String>)>;
    let joined: Vec<Option<DoneV1<T>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if index >= count {
                            return done;
                        }
                        done.push((index, f(index)));
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().ok())
            .collect()
    });
    if joined.iter().any(Option::is_none) {
        return (0..count).map(|_| Err(panicked.to_string())).collect();
    }
    let mut results: Vec<(usize, Result<T, String>)> =
        joined.into_iter().flatten().flatten().collect();
    results.sort_by_key(|entry| entry.0);
    results.into_iter().map(|entry| entry.1).collect()
}

/// A shard file read ahead: the pin and name it was read under, and what
/// `read_shard_file_v1` returned.
type PrefetchedV1 = (PinnedFileV1, String, Result<Vec<u8>, String>);

/// Reads every shard's trajectory files (as named by its receipt, at the
/// schedule positions the receipts give, within the schedule) on a few
/// threads with `read_shard_file_v1`. The merge takes a result only for the
/// same pin and name it reaches in its serial pass, so the checks, their
/// order and the first failure reported are the serial pass's; anything not
/// read ahead is read there as before.
fn prefetch_shard_files_v1(
    shard_directories: &[PathBuf],
    schedule: usize,
) -> Vec<Vec<Option<PrefetchedV1>>> {
    let mut items = Vec::new();
    let mut first_slot = 0usize;
    for (shard, directory) in shard_directories.iter().enumerate() {
        let Ok((_, document)) = read_json_file_v1(&directory.join("collection.json")) else {
            break;
        };
        let Ok(pins) =
            serde_json::from_value::<Vec<PinnedFileV1>>(document["trajectories"].clone())
        else {
            break;
        };
        if first_slot + pins.len() > schedule {
            break;
        }
        for (offset, pin) in pins.iter().enumerate() {
            let name = format!("episode-{:04}.json", first_slot + offset);
            items.push((shard, offset, directory, pin.clone(), name));
        }
        first_slot += pins.len();
    }
    let read = ordered_v1(items.len(), "shard read worker panicked", |index| {
        let (_, _, directory, pin, name) = &items[index];
        Ok(read_shard_file_v1(directory, pin, name))
    });
    let mut prefetched: Vec<Vec<Option<PrefetchedV1>>> =
        shard_directories.iter().map(|_| Vec::new()).collect();
    for ((shard, offset, _, pin, name), result) in items.into_iter().zip(read) {
        let Ok(result) = result else {
            continue;
        };
        let slots = &mut prefetched[shard];
        if slots.len() <= offset {
            slots.resize_with(offset + 1, || None);
        }
        slots[offset] = Some((pin, name, result));
    }
    prefetched
}

/// Merges shard outputs (`shard_directories`, in schedule order) into the
/// whole command's output directory and returns the published receipt.
pub fn merge_shards_v1(
    command: &ExpandedTrainingCommandV1,
    shard_directories: &[PathBuf],
) -> Result<Value, String> {
    let whole = whole_command_v1(command)?;
    ensure(
        !shard_directories.is_empty(),
        "a pooled collection needs a shard",
    )?;
    ensure(
        !whole.output_directory.exists(),
        "output directory already exists",
    )?;
    let schedule = whole.episodes.len();
    let mut first_slot = 0usize;
    let mut reference: Option<Value> = None;
    let mut state_hash: Option<Value> = None;
    let mut trajectory_bytes: Vec<Vec<u8>> = Vec::with_capacity(schedule);
    let mut ledger_entries: Vec<NonNaturalLedgerEntryV1> = Vec::new();
    let mut worker_timings = Vec::new();
    let mut provenance = Vec::with_capacity(shard_directories.len());
    let mut workers_started = 0u64;
    let mut initialization_seconds = 0f64;
    let mut elapsed_seconds = 0f64;
    let mut prefetched = prefetch_shard_files_v1(shard_directories, schedule);
    for (shard_index, directory) in shard_directories.iter().enumerate() {
        let (receipt_bytes, document) = read_json_file_v1(&directory.join("collection.json"))?;
        ensure(
            document["complete"] == true,
            "shard collection is incomplete",
        )?;
        ensure(
            document["source"] == serde_json::to_value(whole.source).map_err(err)?,
            "shard source differs from the collection's",
        )?;
        ensure(
            document.get("collection_sampler").and_then(Value::as_str)
                == whole.collection_sampler.identity_v1(),
            "shard collection sampler differs from the collection's",
        )?;
        ensure(
            document.get("training_math").and_then(Value::as_str)
                == whole.training_math.identity_v1(),
            "shard training math differs from the collection's",
        )?;
        let shard: CollectionShardV1 = serde_json::from_value(document["shard"].clone())
            .map_err(|_| "shard receipt does not name its schedule position".to_string())?;
        ensure(
            shard.first_slot == first_slot && shard.schedule_episodes == schedule,
            "shards are not contiguous in schedule order",
        )?;
        match &state_hash {
            None => state_hash = Some(document["behavior_state_sha256"].clone()),
            Some(hash) => ensure(
                *hash == document["behavior_state_sha256"],
                "shards collected from different learner states",
            )?,
        }
        let mut common = document.clone();
        if let Some(object) = common.as_object_mut() {
            for key in PER_SHARD_KEYS_V1 {
                object.remove(*key);
            }
        }
        match &reference {
            None => reference = Some(common),
            Some(first) => ensure(
                *first == common,
                "shards differ in sampler, training math or contract",
            )?,
        }
        let pins: Vec<PinnedFileV1> =
            serde_json::from_value(document["trajectories"].clone()).map_err(err)?;
        ensure(
            !pins.is_empty() && first_slot + pins.len() <= schedule,
            "shard trajectory count lies outside the schedule",
        )?;
        for (offset, pin) in pins.iter().enumerate() {
            let name = format!("episode-{:04}.json", first_slot + offset);
            let ahead = prefetched[shard_index]
                .get_mut(offset)
                .and_then(Option::take)
                .filter(|(read_pin, read_name, _)| read_pin == pin && *read_name == name);
            trajectory_bytes.push(match ahead {
                Some((_, _, read)) => read?,
                None => read_shard_file_v1(directory, pin, &name)?,
            });
        }
        match document.get("non_natural_ledger") {
            None | Some(Value::Null) => ensure(
                whole.max_non_natural_episode_fraction == 0.0,
                "tolerant shard published no ledger",
            )?,
            Some(pin) => {
                ensure(
                    whole.max_non_natural_episode_fraction > 0.0,
                    "intolerant shard published a ledger",
                )?;
                let pin: PinnedFileV1 = serde_json::from_value(pin.clone()).map_err(err)?;
                let bytes = read_shard_file_v1(directory, &pin, "non-natural.json")?;
                let ledger: NonNaturalLedgerV1 = serde_json::from_slice(&bytes).map_err(err)?;
                ensure(
                    ledger.schema == NON_NATURAL_LEDGER_SCHEMA_V1,
                    "shard ledger schema differs",
                )?;
                for entry in ledger.entries {
                    ensure(
                        (first_slot..first_slot + pins.len()).contains(&entry.slot),
                        "shard ledger names a slot outside its shard",
                    )?;
                    ledger_entries.push(entry);
                }
            }
        }
        if let Some(timings) = document["collection_worker_timings"].as_array() {
            for timing in timings {
                let mut timing = timing.clone();
                timing["worker"] = json!(worker_timings.len());
                worker_timings.push(timing);
            }
        }
        let started = document["collection_workers_started"].as_u64().unwrap_or(0);
        workers_started += started;
        let shard_elapsed = document["collection_elapsed_seconds"]
            .as_f64()
            .unwrap_or(0.0);
        initialization_seconds = initialization_seconds.max(
            document["collection_initialization_seconds"]
                .as_f64()
                .unwrap_or(0.0),
        );
        elapsed_seconds = elapsed_seconds.max(shard_elapsed);
        provenance.push(json!({
            "first_slot": first_slot,
            "episodes": pins.len(),
            "collection_sha256": sha(&receipt_bytes),
            "collection_workers_started": started,
            "collection_elapsed_seconds": shard_elapsed,
        }));
        first_slot += pins.len();
    }
    ensure(
        first_slot == schedule,
        "shards do not cover every episode of the collection",
    )?;
    fs::create_dir(whole.output_directory).map_err(err)?;
    // The same order and cap as one process: entries sorted by slot then
    // attempt, the ledger kept on disk even when the cap aborts.
    ledger_entries.sort_by_key(|entry| (entry.slot, entry.attempt));
    let failed_slots: BTreeSet<usize> = ledger_entries.iter().map(|entry| entry.slot).collect();
    let failed_fraction = failed_slots.len() as f32 / schedule as f32;
    let ledger_pin = if whole.max_non_natural_episode_fraction > 0.0 {
        Some(publish_json(
            whole.output_directory,
            "non-natural.json",
            &NonNaturalLedgerV1 {
                schema: NON_NATURAL_LEDGER_SCHEMA_V1.into(),
                entries: ledger_entries,
            },
        )?)
    } else {
        None
    };
    // Bind every trajectory to the whole command before anything else is
    // published: the requested record envelope, and the scheduled episode
    // (with the seed the ledger re-derives for a retried slot, after the
    // ledger itself is checked against the schedule).
    let seed_overrides = match &ledger_pin {
        Some(pin) => ledgered_retry_seed_overrides_v1(pin, whole.episodes)?,
        None => vec![None; schedule],
    };
    check_trajectories_against_schedule_v1(&trajectory_bytes, &whole, &seed_overrides)?;
    // Each trajectory is its own durable publication; they go out on a few
    // threads, and the first failure in slot order is reported. The receipt
    // is published last, as before.
    let trajectory_bytes: Vec<Arc<Vec<u8>>> = trajectory_bytes.into_iter().map(Arc::new).collect();
    let outputs = ordered_v1(schedule, "trajectory publication worker panicked", |slot| {
        publish_json_shared_v1(
            whole.output_directory,
            &format!("episode-{slot:04}.json"),
            &trajectory_bytes[slot],
        )
    })
    .into_iter()
    .collect::<Result<Vec<_>, String>>()?;
    if whole.max_non_natural_episode_fraction > 0.0 {
        ensure(
            failed_fraction <= whole.max_non_natural_episode_fraction,
            &format!(
                "non-natural episode fraction {failed_fraction} exceeds the configured maximum \
                 {}",
                whole.max_non_natural_episode_fraction
            ),
        )?;
    }
    let mut result = json!({
        "schema":"mtg-kernel-expanded-deck-collection/v1",
        "complete":true,
        "source":whole.source,
        "behavior_state_sha256":state_hash.unwrap_or(Value::Null),
        "trajectories":outputs,
        "collection_backend":POOLED_COLLECTION_BACKEND_V1,
        "collection_workers_requested":whole.workers,
        "collection_workers_started":workers_started,
        "collection_initialization_seconds":initialization_seconds,
        "collection_elapsed_seconds":elapsed_seconds,
        "collection_worker_timings":worker_timings,
        "pooled_shards":provenance,
    });
    if let (Some(object), Some(Value::Object(common))) = (result.as_object_mut(), reference) {
        for (key, value) in common {
            object.insert(key, value);
        }
    }
    if let Some(pin) = ledger_pin {
        result["non_natural_ledger"] = json!(pin);
    }
    publish_json(whole.output_directory, "collection.json", &result)?;
    Ok(result)
}

/// Every trajectory carries the requested envelope and exactly its scheduled
/// episode. Opened and parsed on a few threads; the first failing slot in
/// schedule order is reported.
///
/// Each record is read for the three members the check uses
/// (`ScheduleFieldsV1`): the whole document is parsed exactly as a
/// `serde_json::Value` parse reads it (every number and string in full, the
/// same nesting limit, so the same errors), but only those members are kept.
fn check_trajectories_against_schedule_v1(
    trajectories: &[Vec<u8>],
    whole: &WholeCommandV1<'_>,
    seed_overrides: &[Option<u64>],
) -> Result<(), String> {
    check_trajectories_against_schedule_with_v1(trajectories, whole, seed_overrides, true)
}

/// A JSON value parsed in full, as `serde_json::Value` parses it (through
/// `deserialize_any`, so every number is read as the number `Value` would
/// hold, every string and key is checked, and nesting counts against the
/// parser's limit), and then dropped.
struct DiscardedV1;

impl<'de> Deserialize<'de> for DiscardedV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(DiscardedVisitorV1)
    }
}

struct DiscardedVisitorV1;

impl<'de> serde::de::Visitor<'de> for DiscardedVisitorV1 {
    type Value = DiscardedV1;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<DiscardedV1, E> {
        Ok(DiscardedV1)
    }

    fn visit_i64<E>(self, _: i64) -> Result<DiscardedV1, E> {
        Ok(DiscardedV1)
    }

    fn visit_u64<E>(self, _: u64) -> Result<DiscardedV1, E> {
        Ok(DiscardedV1)
    }

    fn visit_f64<E>(self, _: f64) -> Result<DiscardedV1, E> {
        Ok(DiscardedV1)
    }

    fn visit_str<E>(self, _: &str) -> Result<DiscardedV1, E> {
        Ok(DiscardedV1)
    }

    fn visit_none<E>(self) -> Result<DiscardedV1, E> {
        Ok(DiscardedV1)
    }

    fn visit_unit<E>(self) -> Result<DiscardedV1, E> {
        Ok(DiscardedV1)
    }

    fn visit_some<D: serde::Deserializer<'de>>(self, value: D) -> Result<DiscardedV1, D::Error> {
        DiscardedV1::deserialize(value)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<DiscardedV1, A::Error> {
        while seq.next_element::<DiscardedV1>()?.is_some() {}
        Ok(DiscardedV1)
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<DiscardedV1, A::Error> {
        while map.next_entry::<DiscardedV1, DiscardedV1>()?.is_some() {}
        Ok(DiscardedV1)
    }
}

/// The members of a trajectory record the schedule check reads, as a
/// `serde_json::Value` parse of the whole record gives them: `episode` is
/// `document["episode"]` (null when absent or when the record is no object),
/// the other two `document.get(..)`; a repeated member keeps its last value,
/// as a `Value` object does. Every other member is parsed and dropped.
#[derive(Default)]
struct ScheduleFieldsV1 {
    episode: Value,
    learner_sampler: Option<Value>,
    training_math: Option<Value>,
}

impl<'de> Deserialize<'de> for ScheduleFieldsV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ScheduleFieldsVisitorV1)
    }
}

struct ScheduleFieldsVisitorV1;

impl<'de> serde::de::Visitor<'de> for ScheduleFieldsVisitorV1 {
    type Value = ScheduleFieldsV1;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<ScheduleFieldsV1, E> {
        Ok(ScheduleFieldsV1::default())
    }

    fn visit_i64<E>(self, _: i64) -> Result<ScheduleFieldsV1, E> {
        Ok(ScheduleFieldsV1::default())
    }

    fn visit_u64<E>(self, _: u64) -> Result<ScheduleFieldsV1, E> {
        Ok(ScheduleFieldsV1::default())
    }

    fn visit_f64<E>(self, _: f64) -> Result<ScheduleFieldsV1, E> {
        Ok(ScheduleFieldsV1::default())
    }

    fn visit_str<E>(self, _: &str) -> Result<ScheduleFieldsV1, E> {
        Ok(ScheduleFieldsV1::default())
    }

    fn visit_none<E>(self) -> Result<ScheduleFieldsV1, E> {
        Ok(ScheduleFieldsV1::default())
    }

    fn visit_unit<E>(self) -> Result<ScheduleFieldsV1, E> {
        Ok(ScheduleFieldsV1::default())
    }

    fn visit_some<D: serde::Deserializer<'de>>(
        self,
        value: D,
    ) -> Result<ScheduleFieldsV1, D::Error> {
        DiscardedV1::deserialize(value).map(|_| ScheduleFieldsV1::default())
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> Result<ScheduleFieldsV1, A::Error> {
        while seq.next_element::<DiscardedV1>()?.is_some() {}
        Ok(ScheduleFieldsV1::default())
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(
        self,
        mut map: A,
    ) -> Result<ScheduleFieldsV1, A::Error> {
        let mut fields = ScheduleFieldsV1::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "episode" => fields.episode = map.next_value()?,
                "learner_sampler" => fields.learner_sampler = Some(map.next_value()?),
                "training_math" => fields.training_math = Some(map.next_value()?),
                _ => {
                    map.next_value::<DiscardedV1>()?;
                }
            }
        }
        Ok(fields)
    }
}

/// The members `check_trajectories_against_schedule_v1` reads: through
/// `ScheduleFieldsV1` when `fields_only`, else from a whole `Value` parse
/// (the reference the tests and the merge bench compare with).
fn schedule_fields_v1(
    record: &[u8],
    fields_only: bool,
) -> Result<(Value, Option<String>, Option<String>), String> {
    let string = |value: Option<Value>| value.and_then(|v| v.as_str().map(str::to_owned));
    if fields_only {
        let fields: ScheduleFieldsV1 = serde_json::from_slice(record).map_err(err)?;
        return Ok((
            fields.episode,
            string(fields.learner_sampler),
            string(fields.training_math),
        ));
    }
    let document: Value = serde_json::from_slice(record).map_err(err)?;
    Ok((
        document["episode"].clone(),
        string(document.get("learner_sampler").cloned()),
        string(document.get("training_math").cloned()),
    ))
}

/// [`check_trajectories_against_schedule_v1`]; `fields_only` false parses
/// every record whole (the reference the tests compare with).
fn check_trajectories_against_schedule_with_v1(
    trajectories: &[Vec<u8>],
    whole: &WholeCommandV1<'_>,
    seed_overrides: &[Option<u64>],
    fields_only: bool,
) -> Result<(), String> {
    let (episodes, envelope) = (whole.episodes, whole.record_envelope);
    ensure(
        trajectories.len() == episodes.len() && seed_overrides.len() == episodes.len(),
        "collection episode count differs",
    )?;
    let check = |slot: usize| -> Result<(), String> {
        let bytes = &trajectories[slot];
        ensure(
            record_envelope_v1::is_sealed_v1(bytes) != envelope.is_none(),
            &format!("shard trajectory {slot} is not in the requested record envelope"),
        )?;
        let record = record_envelope_v1::open_v1(bytes, MAX_FILE_BYTES)?;
        let (episode, learner_sampler, training_math) = schedule_fields_v1(&record, fields_only)?;
        let mut expected = episodes[slot].clone();
        if let Some(seed) = seed_overrides[slot] {
            expected.seed = seed;
        }
        ensure(
            episode == serde_json::to_value(&expected).map_err(err)?,
            &format!("shard trajectory {slot} does not carry its scheduled episode"),
        )?;
        ensure(
            learner_sampler.as_deref() == whole.collection_sampler.identity_v1()
                && training_math.as_deref() == whole.training_math.identity_v1(),
            &format!("shard trajectory {slot} differs in sampler or training math"),
        )
    };
    let workers = std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .clamp(1, 16)
        .min(trajectories.len());
    let results: Vec<Result<(), String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let check = &check;
                scope.spawn(move || {
                    (worker..trajectories.len())
                        .step_by(workers)
                        .map(|slot| (slot, check(slot)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all: Vec<(usize, Result<(), String>)> = handles
            .into_iter()
            .flat_map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    vec![(0, Err("trajectory check worker panicked".to_string()))]
                })
            })
            .collect();
        all.sort_by_key(|entry| entry.0);
        all.into_iter().map(|entry| entry.1).collect()
    });
    results.into_iter().collect()
}

/// A command file of at most 16 MiB (the trainer's config bound).
fn read_command_file_v1(path: &Path) -> Result<ExpandedTrainingCommandV1, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    ensure(bytes.len() <= 16 * 1024 * 1024, "command exceeds 16 MiB")?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

/// Executes one shard command file: only a `collect_parallel` command that
/// names its schedule position is accepted.
pub fn execute_shard_file_v1(path: &Path) -> Result<Value, String> {
    let command = read_command_file_v1(path)?;
    ensure(
        matches!(
            command,
            ExpandedTrainingCommandV1::CollectParallel { shard: Some(_), .. }
        ),
        "a shard file must hold a collect_parallel command with its shard",
    )?;
    execute_v1(command)
}

/// [`merge_shards_v1`] for the whole command in `command_path`.
pub fn merge_shards_file_v1(
    command_path: &Path,
    shard_directories: &[PathBuf],
) -> Result<Value, String> {
    merge_shards_v1(&read_command_file_v1(command_path)?, shard_directories)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck_v1(id: &str) -> ExpandedDeckListV1 {
        let registration =
            crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
        let cards = registration.registered_configuration();
        ExpandedDeckListV1 {
            label: id.into(),
            mainboard: cards.mainboard().to_vec(),
            sideboard: cards.sideboard().to_vec(),
        }
    }

    /// A synthetic fresh source and `count` real Affinity-Terror episodes.
    fn fixture_v1(
        label: &str,
        count: u64,
    ) -> (PathBuf, ExpandedModelSourceV1, Vec<ExpandedEpisodeV1>) {
        let feature_identity = crate::sideboard_play_policy_v1::FRESH_FEATURE_IDENTITY_V3;
        let root =
            std::env::temp_dir().join(format!("pooled-collection-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
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
            s_learner: None,
        };
        let decks = [deck_v1("Affinity"), deck_v1("Terror")];
        // Seeds unique to each test: the forced non-natural seed set is
        // process-wide and tests run concurrently.
        let base = 2_026_112_000 + 1_000 * u64::from(label.bytes().fold(0u8, u8::wrapping_add));
        let episodes = (0..count)
            .map(|index| ExpandedEpisodeV1 {
                id: format!("{label}-{index}"),
                seed: base + index,
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
                london_keep_offset: None,
                priority_windows: None,
                priority_pass_offset: None,
            })
            .collect();
        (root, source, episodes)
    }

    fn whole_command_for_test_v1(
        source: &ExpandedModelSourceV1,
        episodes: &[ExpandedEpisodeV1],
        fraction: f32,
        record_envelope: RecordEnvelopeV1,
        output_directory: PathBuf,
    ) -> ExpandedTrainingCommandV1 {
        ExpandedTrainingCommandV1::CollectParallel {
            source: source.clone(),
            episodes: episodes.to_vec(),
            workers: 2,
            max_non_natural_episode_fraction: fraction,
            collection_sampler: CollectionSamplerV1::Legacy,
            training_math: TrainingMathV1::PlatformLibm,
            ex1: None,
            record_envelope,
            shard: None,
            output_directory,
        }
    }

    fn pins_of_v1(receipt: &Value) -> Vec<PinnedFileV1> {
        serde_json::from_value(receipt["trajectories"].clone()).unwrap()
    }

    fn ledger_of_v1(receipt: &Value) -> Value {
        let pin: PinnedFileV1 =
            serde_json::from_value(receipt["non_natural_ledger"].clone()).unwrap();
        read_pinned(&pin).unwrap()
    }

    /// Shards of 3, 1 and 3 episodes, each on two workers, merge into the
    /// trajectories one two-worker process publishes for the same seven
    /// episodes: same bytes, same order, same ledger (one slot in the middle
    /// shard fails its first attempt), and a receipt the run loop's checks
    /// read the same way (source, state, sampler, training math).
    #[test]
    fn pooled_shards_merge_into_the_single_process_collection() {
        let (root, source, episodes) = fixture_v1("identity", 7);
        let forced = episodes[3].seed;
        for envelope in [RecordEnvelopeV1::default(), RecordEnvelopeV1::Zstd1] {
            let tag = if envelope.is_none() { "plain" } else { "zstd1" };
            set_force_non_natural_seeds_for_test_v1([forced]);
            let single = execute_v1(whole_command_for_test_v1(
                &source,
                &episodes,
                0.5,
                envelope,
                root.join(format!("{tag}-single")),
            ));
            let whole = whole_command_for_test_v1(
                &source,
                &episodes,
                0.5,
                envelope,
                root.join(format!("{tag}-merged")),
            );
            let specs: Vec<_> = [3usize, 1, 3]
                .iter()
                .enumerate()
                .map(|(index, &count)| ShardSpecV1 {
                    episodes: count,
                    workers: 2,
                    output_directory: root.join(format!("{tag}-shard-{index}")),
                })
                .collect();
            let shards = shard_commands_v1(&whole, &specs).unwrap();
            for shard in shards {
                execute_v1(shard).unwrap();
            }
            clear_force_non_natural_seeds_for_test_v1([forced]);
            let single = single.unwrap();
            let directories: Vec<_> = specs.iter().map(|s| s.output_directory.clone()).collect();
            let merged = merge_shards_v1(&whole, &directories).unwrap();
            let (single_pins, merged_pins) = (pins_of_v1(&single), pins_of_v1(&merged));
            assert_eq!(single_pins.len(), 7);
            for (a, b) in single_pins.iter().zip(&merged_pins) {
                assert_eq!(a.sha256, b.sha256, "{tag}: trajectory bytes differ");
                assert_eq!(a.path.file_name(), b.path.file_name());
                assert_eq!(fs::read(&a.path).unwrap(), fs::read(&b.path).unwrap());
            }
            assert_eq!(
                ledger_of_v1(&single),
                ledger_of_v1(&merged),
                "{tag}: ledger differs"
            );
            assert_eq!(ledger_of_v1(&merged)["entries"][0]["slot"], 3);
            for key in [
                "schema",
                "complete",
                "source",
                "behavior_state_sha256",
                "collection_sampler",
                "training_math",
                "input_contract",
            ] {
                assert_eq!(single.get(key), merged.get(key), "{tag}: receipt key {key}");
            }
            assert_eq!(merged["collection_backend"], POOLED_COLLECTION_BACKEND_V1);
            assert_eq!(merged["pooled_shards"].as_array().unwrap().len(), 3);
            let on_disk: Value = serde_json::from_slice(
                &fs::read(root.join(format!("{tag}-merged/collection.json"))).unwrap(),
            )
            .unwrap();
            assert_eq!(on_disk, merged);
        }
        fs::remove_dir_all(root).unwrap();
    }

    /// The fraction cap belongs to the whole schedule: a one-episode shard
    /// whose slot needed a retry is admitted (a shard never applies the cap),
    /// and the merge applies the whole collection's cap, keeping the ledger.
    #[test]
    fn the_non_natural_cap_applies_to_the_whole_schedule_not_a_shard() {
        let (root, source, episodes) = fixture_v1("cap", 4);
        let forced = episodes[2].seed;
        let run = |fraction: f32, tag: &str| {
            let whole = whole_command_for_test_v1(
                &source,
                &episodes,
                fraction,
                RecordEnvelopeV1::default(),
                root.join(format!("{tag}-merged")),
            );
            let specs: Vec<_> = [2usize, 1, 1]
                .iter()
                .enumerate()
                .map(|(index, &count)| ShardSpecV1 {
                    episodes: count,
                    workers: 2,
                    output_directory: root.join(format!("{tag}-shard-{index}")),
                })
                .collect();
            set_force_non_natural_seeds_for_test_v1([forced]);
            for shard in shard_commands_v1(&whole, &specs).unwrap() {
                // The one-episode shard holding the retried slot: fraction 1.0
                // of the shard, admitted because a shard applies no cap.
                execute_v1(shard).unwrap();
            }
            clear_force_non_natural_seeds_for_test_v1([forced]);
            let directories: Vec<_> = specs.iter().map(|s| s.output_directory.clone()).collect();
            (
                merge_shards_v1(&whole, &directories),
                root.join(format!("{tag}-merged")),
            )
        };
        let (admitted, _) = run(0.5, "admitted");
        let admitted = admitted.unwrap();
        assert_eq!(ledger_of_v1(&admitted)["entries"][0]["slot"], 2);
        let (capped, directory) = run(0.1, "capped");
        let error = capped.unwrap_err();
        assert!(
            error.contains("non-natural episode fraction"),
            "got: {error}"
        );
        assert!(directory.join("non-natural.json").exists());
        assert!(!directory.join("collection.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    /// The merge refuses shards that do not form the schedule: a gap, a
    /// changed trajectory, a shard from another learner state.
    #[test]
    fn the_merge_refuses_shards_that_do_not_form_the_schedule() {
        let (root, source, episodes) = fixture_v1("refuse", 3);
        let whole = whole_command_for_test_v1(
            &source,
            &episodes,
            0.0,
            RecordEnvelopeV1::default(),
            root.join("merged"),
        );
        let specs: Vec<_> = [1usize, 2]
            .iter()
            .enumerate()
            .map(|(index, &count)| ShardSpecV1 {
                episodes: count,
                workers: 2,
                output_directory: root.join(format!("shard-{index}")),
            })
            .collect();
        for shard in shard_commands_v1(&whole, &specs).unwrap() {
            execute_v1(shard).unwrap();
        }
        let first = specs[0].output_directory.clone();
        let second = specs[1].output_directory.clone();
        // Out of order, or a shard missing.
        let error = merge_shards_v1(&whole, &[second.clone(), first.clone()]).unwrap_err();
        assert!(error.contains("contiguous"), "got: {error}");
        let error = merge_shards_v1(&whole, std::slice::from_ref(&first)).unwrap_err();
        assert!(error.contains("cover every episode"), "got: {error}");
        // A changed trajectory byte.
        let path = second.join("episode-0002.json");
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        let middle = changed.len() / 2;
        changed[middle] ^= 1;
        fs::write(&path, &changed).unwrap();
        let error = merge_shards_v1(&whole, &[first.clone(), second.clone()]).unwrap_err();
        assert!(error.contains("SHA differs"), "got: {error}");
        fs::write(&path, &original).unwrap();
        assert!(
            !root.join("merged").exists(),
            "a refused merge publishes nothing"
        );
        merge_shards_v1(&whole, &[first.clone(), second.clone()]).unwrap();
        // Coherent shards of another command (same source, other episodes)
        // are refused: every trajectory is bound to its scheduled episode.
        let mut foreign_episodes = episodes.clone();
        for episode in &mut foreign_episodes {
            episode.seed += 500;
        }
        let foreign = whole_command_for_test_v1(
            &source,
            &foreign_episodes,
            0.0,
            RecordEnvelopeV1::default(),
            root.join("unused"),
        );
        let foreign_specs: Vec<_> = specs
            .iter()
            .enumerate()
            .map(|(index, spec)| ShardSpecV1 {
                output_directory: root.join(format!("foreign-{index}")),
                ..spec.clone()
            })
            .collect();
        for shard in shard_commands_v1(&foreign, &foreign_specs).unwrap() {
            execute_v1(shard).unwrap();
        }
        let foreign_dirs: Vec<_> = foreign_specs
            .iter()
            .map(|s| s.output_directory.clone())
            .collect();
        let again = whole_command_for_test_v1(
            &source,
            &episodes,
            0.0,
            RecordEnvelopeV1::default(),
            root.join("merged-2"),
        );
        let error = merge_shards_v1(&again, &foreign_dirs).unwrap_err();
        assert!(error.contains("scheduled episode"), "got: {error}");
        // Plain shards do not merge into a command that asks for zstd1.
        let sealed = whole_command_for_test_v1(
            &source,
            &episodes,
            0.0,
            RecordEnvelopeV1::Zstd1,
            root.join("merged-3"),
        );
        let error = merge_shards_v1(&sealed, &[first.clone(), second.clone()]).unwrap_err();
        assert!(error.contains("record envelope"), "got: {error}");
        // Consistent legacy-sampler shards do not merge into a command that
        // asks for another sampler.
        let mut unclamped = whole_command_for_test_v1(
            &source,
            &episodes,
            0.0,
            RecordEnvelopeV1::default(),
            root.join("merged-4"),
        );
        if let ExpandedTrainingCommandV1::CollectParallel {
            collection_sampler, ..
        } = &mut unclamped
        {
            *collection_sampler = CollectionSamplerV1::UnclampedSoftmaxF64IcdfU53V1;
        }
        let error = merge_shards_v1(&unclamped, &[first, second]).unwrap_err();
        assert!(error.contains("collection sampler"), "got: {error}");
        // Shard plans that do not cover the schedule are refused up front.
        let bad = vec![ShardSpecV1 {
            episodes: 2,
            workers: 2,
            output_directory: root.join("bad"),
        }];
        assert!(shard_commands_v1(&whole, &bad).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    /// The whole command passes the ordinary collector's preflight before any
    /// split: an episode id repeated across a shard boundary, or a worker count
    /// one process would refuse, is refused when sharding and when merging.
    #[test]
    fn a_whole_command_the_ordinary_collector_refuses_is_refused_pooled() {
        let (root, source, mut episodes) = fixture_v1("preflight", 4);
        episodes[3].id = episodes[0].id.clone();
        let whole = whole_command_for_test_v1(
            &source,
            &episodes,
            0.0,
            RecordEnvelopeV1::default(),
            root.join("merged"),
        );
        let specs = vec![
            ShardSpecV1 {
                episodes: 2,
                workers: 2,
                output_directory: root.join("a"),
            },
            ShardSpecV1 {
                episodes: 2,
                workers: 2,
                output_directory: root.join("b"),
            },
        ];
        let error = shard_commands_v1(&whole, &specs).unwrap_err();
        assert!(error.contains("duplicate episode id"), "got: {error}");
        let error = merge_shards_v1(&whole, &[root.join("a"), root.join("b")]).unwrap_err();
        assert!(error.contains("duplicate episode id"), "got: {error}");
        let (_, _, episodes) = fixture_v1("preflight-workers", 2);
        for workers in [0usize, 1, 1025] {
            let mut command = whole_command_for_test_v1(
                &source,
                &episodes,
                0.0,
                RecordEnvelopeV1::default(),
                root.join("merged"),
            );
            if let ExpandedTrainingCommandV1::CollectParallel { workers: w, .. } = &mut command {
                *w = workers;
            }
            assert!(
                shard_commands_v1(&command, &specs[..1]).is_err(),
                "workers {workers} must be refused"
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    /// A shard command round-trips through its wire form, and a whole
    /// command without a shard keeps its existing bytes (no `shard` key).
    #[test]
    fn shard_commands_round_trip_and_whole_commands_keep_their_bytes() {
        let (root, source, episodes) = fixture_v1("wire", 3);
        let whole = whole_command_for_test_v1(
            &source,
            &episodes,
            0.0,
            RecordEnvelopeV1::default(),
            root.join("merged"),
        );
        let wire = serde_json::to_value(&whole).unwrap();
        assert!(wire.get("shard").is_none());
        let specs = vec![
            ShardSpecV1 {
                episodes: 2,
                workers: 4,
                output_directory: root.join("a"),
            },
            ShardSpecV1 {
                episodes: 1,
                workers: 2,
                output_directory: root.join("b"),
            },
        ];
        let shards = shard_commands_v1(&whole, &specs).unwrap();
        let wire = serde_json::to_value(&shards[1]).unwrap();
        assert_eq!(
            wire["shard"],
            json!({"first_slot": 2, "schedule_episodes": 3})
        );
        assert_eq!(wire["episodes"].as_array().unwrap().len(), 1);
        let back: ExpandedTrainingCommandV1 = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(back).unwrap(), wire);
        let _ = fs::remove_dir_all(root);
    }

    /// The members the schedule check reads are those a whole `Value` parse
    /// gives, and every record that parse refuses is refused with its error:
    /// absent, repeated or non-string members, records that are no object,
    /// numbers out of range, bad strings, too deep a nesting, trailing data.
    #[test]
    fn the_schedule_fields_are_what_a_whole_parse_gives_errors_included() {
        let deep = format!("{{\"x\":{}1{}}}", "[".repeat(200), "]".repeat(200));
        let records: Vec<Vec<u8>> = vec![
            br#"{"episode":{"id":"a","seed":1},"learner_sampler":"s","training_math":"m","steps":[1,2.5,-3,"x",null,true,{"k":[]}]}"#.to_vec(),
            br#"{"steps":[1,2,3]}"#.to_vec(),
            br#"{"episode":1,"episode":{"id":"last"},"learner_sampler":7,"training_math":null}"#.to_vec(),
            br#"[{"episode":{"id":"a"}}]"#.to_vec(),
            br#""episode""#.to_vec(),
            br#"12"#.to_vec(),
            br#"{"episode":{"london_keep_offset":0.0004882813082076609},"x":1e400}"#.to_vec(),
            br#"{"episode":{},"x":"\ud800"}"#.to_vec(),
            b"{\"episode\":{},\"x\":\"\xff\"}".to_vec(),
            br#"{"episode":{},"x":"tab	here"}"#.to_vec(),
            br#"{"episode":{}} trailing"#.to_vec(),
            br#"{"episode":{},"x":[1,2,}"#.to_vec(),
            deep.into_bytes(),
            br#"{"episode":{"a":-0.0,"b":18446744073709551615,"c":-9223372036854775808,"d":1.7976931348623157e308}}"#.to_vec(),
        ];
        for record in &records {
            assert_eq!(
                schedule_fields_v1(record, true),
                schedule_fields_v1(record, false),
                "{}",
                String::from_utf8_lossy(record)
            );
        }
    }

    /// Merge timing at a production size (`MERGE_BENCH_EPISODES`, 480 by
    /// default, in two shards): one real game's sealed record is replicated
    /// into every slot with its scheduled episode, then the merge runs, and
    /// each of its parts runs alone the old way and the new way. Prints one
    /// JSON line of seconds.
    #[test]
    #[ignore = "timing: run by hand with --nocapture"]
    fn merge_timing_at_production_size() {
        let count: usize = std::env::var("MERGE_BENCH_EPISODES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(480);
        let (root, source, episodes) = fixture_v1("merge-timing", 1);
        let envelope = RecordEnvelopeV1::Zstd1;
        let template = whole_command_for_test_v1(
            &source,
            &episodes,
            0.0,
            envelope,
            root.join("template-whole"),
        );
        let spec = ShardSpecV1 {
            episodes: 1,
            workers: 2,
            output_directory: root.join("template"),
        };
        let shard = shard_commands_v1(&template, &[spec]).unwrap().remove(0);
        let shard_receipt = execute_v1(shard).unwrap();
        let pin = pins_of_v1(&shard_receipt).remove(0);
        let record = record_envelope_v1::open_v1(&fs::read(&pin.path).unwrap(), MAX_FILE_BYTES)
            .unwrap()
            .into_owned();
        let mut document: Value = serde_json::from_slice(&record).unwrap();
        let schedule: Vec<ExpandedEpisodeV1> = (0..count)
            .map(|slot| {
                let mut episode = episodes[0].clone();
                episode.id = format!("merge-timing-{slot}");
                episode.seed += slot as u64;
                episode
            })
            .collect();
        let whole =
            whole_command_for_test_v1(&source, &schedule, 0.0, envelope, root.join("collect"));
        let halves = [count / 2, count - count / 2];
        let mut directories = Vec::new();
        let mut first_slot = 0;
        let mut compressed = 0usize;
        let mut opened = 0usize;
        for (index, size) in halves.iter().enumerate() {
            let directory = root.join(format!("shard-{index}"));
            fs::create_dir_all(&directory).unwrap();
            let mut pins = Vec::new();
            for (slot, episode) in schedule.iter().enumerate().skip(first_slot).take(*size) {
                document["episode"] = serde_json::to_value(episode).unwrap();
                let bytes = serde_json::to_vec(&document).unwrap();
                opened += bytes.len();
                let sealed = envelope.seal_v1(&bytes).unwrap().into_owned();
                compressed += sealed.len();
                let name = format!("episode-{slot:04}.json");
                fs::write(directory.join(&name), &sealed).unwrap();
                pins.push(PinnedFileV1 {
                    path: directory.join(&name),
                    sha256: sha(&sealed),
                });
            }
            let mut receipt = shard_receipt.clone();
            receipt["trajectories"] = serde_json::to_value(&pins).unwrap();
            receipt["shard"] = json!({"first_slot": first_slot, "schedule_episodes": count});
            fs::write(
                directory.join("collection.json"),
                serde_json::to_vec(&receipt).unwrap(),
            )
            .unwrap();
            directories.push(directory);
            first_slot += size;
        }
        let time = |f: &mut dyn FnMut()| {
            let started = std::time::Instant::now();
            f();
            started.elapsed().as_secs_f64()
        };
        let merge = time(&mut || {
            merge_shards_v1(&whole, &directories).unwrap();
        });
        let names: Vec<(usize, PathBuf, PinnedFileV1, String)> = directories
            .iter()
            .enumerate()
            .flat_map(|(index, directory)| {
                let receipt: Value =
                    serde_json::from_slice(&fs::read(directory.join("collection.json")).unwrap())
                        .unwrap();
                pins_of_v1(&receipt)
                    .into_iter()
                    .map(move |pin| {
                        let name = pin.path.file_name().unwrap().to_str().unwrap().to_owned();
                        (index, directory.clone(), pin, name)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut bytes = Vec::new();
        let serial_read = time(&mut || {
            bytes = names
                .iter()
                .map(|(_, directory, pin, name)| read_shard_file_v1(directory, pin, name).unwrap())
                .collect();
        });
        let parallel_read = time(&mut || {
            prefetch_shard_files_v1(&directories, count);
        });
        let open_only = time(&mut || {
            for sealed in &bytes {
                record_envelope_v1::open_v1(sealed, MAX_FILE_BYTES).unwrap();
            }
        });
        let seeds = vec![None; count];
        let whole_view = whole_command_v1(&whole).unwrap();
        let check_value = time(&mut || {
            check_trajectories_against_schedule_with_v1(&bytes, &whole_view, &seeds, false)
                .unwrap();
        });
        let check_fields = time(&mut || {
            check_trajectories_against_schedule_with_v1(&bytes, &whole_view, &seeds, true).unwrap();
        });
        let shared: Vec<Arc<Vec<u8>>> = bytes.iter().cloned().map(Arc::new).collect();
        let serial_dir = root.join("publish-serial");
        let parallel_dir = root.join("publish-parallel");
        fs::create_dir_all(&serial_dir).unwrap();
        fs::create_dir_all(&parallel_dir).unwrap();
        let serial_publish = time(&mut || {
            for (slot, data) in shared.iter().enumerate() {
                publish_json_shared_v1(&serial_dir, &format!("episode-{slot:04}.json"), data)
                    .unwrap();
            }
        });
        let parallel_publish = time(&mut || {
            for result in ordered_v1(count, "panicked", |slot| {
                publish_json_shared_v1(
                    &parallel_dir,
                    &format!("episode-{slot:04}.json"),
                    &shared[slot],
                )
            }) {
                result.unwrap();
            }
        });
        println!(
            "{}",
            json!({
                "episodes": count,
                "compressed_bytes": compressed,
                "opened_bytes": opened,
                "workers": merge_workers_v1(count),
                "merge_seconds": merge,
                "read_hash_serial_seconds": serial_read,
                "read_hash_parallel_seconds": parallel_read,
                "zstd_open_serial_seconds": open_only,
                "check_whole_value_parse_seconds": check_value,
                "check_fields_parse_seconds": check_fields,
                "publish_serial_seconds": serial_publish,
                "publish_parallel_seconds": parallel_publish,
                "old_merge_estimate_seconds": serial_read + check_value + serial_publish,
            })
        );
        fs::remove_dir_all(root).unwrap();
    }
}
