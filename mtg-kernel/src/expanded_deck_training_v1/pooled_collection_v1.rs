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
    output_directory: &'a Path,
}

fn whole_command_v1(command: &ExpandedTrainingCommandV1) -> Result<WholeCommandV1<'_>, String> {
    match command {
        ExpandedTrainingCommandV1::CollectParallel {
            source,
            episodes,
            workers,
            max_non_natural_episode_fraction,
            ex1,
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
            Ok(WholeCommandV1 {
                source,
                episodes,
                workers: *workers,
                max_non_natural_episode_fraction: *max_non_natural_episode_fraction,
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
    for directory in shard_directories {
        let (receipt_bytes, document) = read_json_file_v1(&directory.join("collection.json"))?;
        ensure(
            document["complete"] == true,
            "shard collection is incomplete",
        )?;
        ensure(
            document["source"] == serde_json::to_value(whole.source).map_err(err)?,
            "shard source differs from the collection's",
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
            trajectory_bytes.push(read_shard_file_v1(directory, pin, &name)?);
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
    let mut outputs = Vec::with_capacity(schedule);
    for (slot, bytes) in trajectory_bytes.into_iter().enumerate() {
        let name = format!("episode-{slot:04}.json");
        outputs.push(publish_json_shared_v1(
            whole.output_directory,
            &name,
            &Arc::new(bytes),
        )?);
    }
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
        let error = merge_shards_v1(&whole, &[first.clone()]).unwrap_err();
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
        merge_shards_v1(&whole, &[first, second]).unwrap();
        // Shard plans that do not cover the schedule are refused up front.
        let bad = vec![ShardSpecV1 {
            episodes: 2,
            workers: 2,
            output_directory: root.join("bad"),
        }];
        assert!(shard_commands_v1(&whole, &bad).is_err());
        fs::remove_dir_all(root).unwrap();
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
}
