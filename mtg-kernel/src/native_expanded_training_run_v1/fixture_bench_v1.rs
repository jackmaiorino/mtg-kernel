//! The opt-in local benchmark fixture for the native training run (#595,
//! under #594's benchmark contract). It packages the source-generated net-8
//! fixture the run's own tests use: synthetic, valid FlatV4 weights
//! (`write_synthetic_fresh_source_with_parameters_v1`, a real deterministic
//! net-8 with real hashes) and the checked-in Affinity and Terror decks, in
//! ordinary self-play against the current learner. No production checkpoint
//! or private input is read.
//!
//! [`write_fixture_v1`] writes, under a fresh root:
//! - `source/` and `descriptor.json`: the synthetic fresh source;
//! - `cases/fresh/config.json`: `updates` iterations of `games` episodes each
//!   from that fresh source (Adam step 0);
//! - with `warmup_updates > 0`, `warmup/run/` (an in-process run of the first
//!   `warmup_updates` iterations of the same schedule), its final checkpoint
//!   copied to `warm/checkpoint.json`, and `cases/warm/config.json`: the next
//!   `updates` iterations of the schedule resumed from that warmed state;
//! - with `pool_members`, `pool/member-NN/checkpoint.json`: generated pool
//!   members (`pool_fixture_v1`), distinct checkpoints over the fresh source's
//!   import, and for each pool size `n` a case `cases/poolN/config.json`: the
//!   fresh schedule with every slot playing a fixed opponent, the member of
//!   the first `n` that `pool_fixture_v1::draw_v1` draws for it (#599);
//! - `fixture.json`: the options, the generator's build identity and every
//!   case's pins.
//!
//! Every case writes its run to the fixed `work/CASE/run`, so the raw files of
//! any two runs of a case compare byte for byte. The benchmark driver
//! (`tools/native_fixture_bench/bench.py`) times the production binary on
//! each case in fresh processes and moves each run out of `work/` after it;
//! nothing here is timed. Episode `g` of iteration `i` has seed
//! `seed + i * games + g`, learner seat 0 (Affinity) and starting player
//! `g % 2`.

use super::*;
use crate::expanded_deck_training_v1::write_synthetic_fresh_source_with_parameters_v1;
use crate::sideboard_play_policy_v1::{FrozenPlayObservationTransferV3, FRESH_FEATURE_IDENTITY_V4};

pub(super) const FIXTURE_SCHEMA_V1: &str = "mtg-kernel-native-fixture/v1";
const DECKS_V1: [&str; 2] = ["Affinity", "Terror"];
const LEARNING_RATE_V1: f32 = 0.0003;
const VALUE_COEFFICIENT_V1: f32 = 0.5;
const GAE_V1: ExpandedLossSelectionV1 = ExpandedLossSelectionV1::GaeAdvantageValueV1 {
    gamma: 1.0,
    lambda: 0.9,
    entropy_coefficient: 0.0,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FixtureOptionsV1 {
    pub(super) root: PathBuf,
    pub(super) seed: u64,
    pub(super) updates: usize,
    pub(super) games: usize,
    /// Iterations run in process to make the warmed state; 0 writes no warm
    /// case.
    pub(super) warmup_updates: usize,
    pub(super) collection_workers: usize,
    pub(super) preparation_workers: usize,
    /// Pool case sizes, increasing: a `poolN` case for each, drawing from
    /// the first `N` generated members. Empty writes no member and no pool
    /// case.
    pub(super) pool_members: Vec<usize>,
}

impl FixtureOptionsV1 {
    /// The ignored entry point's options: `MTG_NATIVE_FIXTURE_ROOT` (required,
    /// absolute, absent or empty) and the optional `_SEED` (2026091601),
    /// `_UPDATES` (2), `_GAMES` (2), `_WARMUP_UPDATES` (`_UPDATES`),
    /// `_COLLECTION_WORKERS` (1), `_PREPARATION_WORKERS` (1) and
    /// `_POOL_MEMBERS` (comma-separated pool sizes; none).
    fn from_env_v1() -> Result<Self, String> {
        let number = |name: &str, default: u64| -> Result<u64, String> {
            match std::env::var(format!("MTG_NATIVE_FIXTURE_{name}")) {
                Ok(text) => text
                    .parse::<u64>()
                    .map_err(|e| format!("MTG_NATIVE_FIXTURE_{name}: {e}")),
                Err(std::env::VarError::NotPresent) => Ok(default),
                Err(error) => Err(format!("MTG_NATIVE_FIXTURE_{name}: {error}")),
            }
        };
        let root = PathBuf::from(
            std::env::var_os("MTG_NATIVE_FIXTURE_ROOT")
                .ok_or("MTG_NATIVE_FIXTURE_ROOT is required")?,
        );
        let updates = number("UPDATES", 2)? as usize;
        let pool_members = match std::env::var("MTG_NATIVE_FIXTURE_POOL_MEMBERS") {
            Ok(text) if !text.is_empty() => text
                .split(',')
                .map(|size| size.trim().parse::<usize>().map_err(err))
                .collect::<Result<Vec<_>, String>>()
                .map_err(|e| format!("MTG_NATIVE_FIXTURE_POOL_MEMBERS: {e}"))?,
            Ok(_) | Err(std::env::VarError::NotPresent) => Vec::new(),
            Err(error) => return Err(format!("MTG_NATIVE_FIXTURE_POOL_MEMBERS: {error}")),
        };
        Ok(Self {
            root,
            seed: number("SEED", 2_026_091_601)?,
            updates,
            games: number("GAMES", 2)? as usize,
            warmup_updates: number("WARMUP_UPDATES", updates as u64)? as usize,
            collection_workers: number("COLLECTION_WORKERS", 1)? as usize,
            preparation_workers: number("PREPARATION_WORKERS", 1)? as usize,
            pool_members,
        })
    }

    fn validate_v1(&self) -> Result<(), String> {
        check(self.root.is_absolute(), "fixture root must be absolute")?;
        check(
            (1..=64).contains(&self.updates) && (1..=64).contains(&self.games),
            "fixture updates and games must be 1 to 64",
        )?;
        check(self.warmup_updates <= 64, "fixture warm-up is at most 64")?;
        check(
            self.pool_members.iter().all(|size| (1..=64).contains(size))
                && self.pool_members.windows(2).all(|pair| pair[0] < pair[1]),
            "fixture pool sizes must increase, each 1 to 64",
        )?;
        check(
            (self.warmup_updates + self.updates) as u64 * self.games as u64 <= u64::MAX - self.seed,
            "fixture seeds overflow",
        )
    }
}

fn deck_list_v1(id: &str) -> crate::expanded_deck_training_v1::ExpandedDeckListV1 {
    let registration = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id)
        .unwrap_or_else(|error| panic!("checked-in deck {id}: {error:?}"));
    let cards = registration.registered_configuration();
    crate::expanded_deck_training_v1::ExpandedDeckListV1 {
        label: id.into(),
        mainboard: cards.mainboard().to_vec(),
        sideboard: cards.sideboard().to_vec(),
    }
}

/// The schedule's iterations `first..first + count`.
fn iterations_v1(
    options: &FixtureOptionsV1,
    first: usize,
    count: usize,
) -> Vec<ExpandedRunIterationV1> {
    let decks = DECKS_V1.map(deck_list_v1);
    (first..first + count)
        .map(|iteration| ExpandedRunIterationV1 {
            episodes: (0..options.games)
                .map(|game| ScheduledExpandedEpisodeV1 {
                    episode: ExpandedEpisodeV1 {
                        id: format!("fixture-{iteration:04}-{game:02}"),
                        seed: options.seed + (iteration * options.games + game) as u64,
                        starting_player: (game % 2) as u8,
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
                    },
                    opponent: ExpandedOpponentAssignmentV1::Current,
                })
                .collect(),
        })
        .collect()
}

fn config_v1(
    options: &FixtureOptionsV1,
    initial_source: ExpandedModelSourceV1,
    iterations: Vec<ExpandedRunIterationV1>,
    output_directory: PathBuf,
) -> NativeExpandedTrainingRunV1 {
    NativeExpandedTrainingRunV1 {
        schema: SCHEMA.into(),
        initial_source,
        opponents: vec![],
        iterations,
        record_format: Default::default(),
        record_envelope: Default::default(),
        learning_rate: LEARNING_RATE_V1,
        value_coefficient: VALUE_COEFFICIENT_V1,
        update_backend: ExpandedUpdateBackendV1::Cpu,
        loss_selection: GAE_V1,
        update_backward_execution: UpdateBackwardExecutionV1::Sequential,
        collection_workers: options.collection_workers,
        preparation_workers: options.preparation_workers,
        max_non_natural_episode_fraction: 0.0,
        max_prepared_tensor_mebibytes: DEFAULT_MAX_PREPARED_TENSOR_MEBIBYTES,
        collection_sampler: CollectionSamplerV1::Legacy,
        training_math: TrainingMathV1::PlatformLibm,
        line_b: None,
        e_supported: None,
        soft_q: None,
        search_targets: None,
        ex1: None,
        record_process_cpu: false,
        chain_ledger: None,
        // Strict rereads, the default; resident reads (#592) are opt-in.
        verified_reads: Default::default(),
        output_directory,
    }
}

fn write_pinned_v1(path: &Path, bytes: &[u8]) -> Result<PinnedFileV1, String> {
    fs::create_dir_all(path.parent().ok_or("fixture file has no parent")?).map_err(err)?;
    fs::write(path, bytes).map_err(err)?;
    Ok(PinnedFileV1 {
        path: path.to_path_buf(),
        sha256: digest(bytes),
    })
}

/// One case: its config, written and pinned, and its manifest entry.
fn write_case_v1(
    root: &Path,
    name: &str,
    config: &NativeExpandedTrainingRunV1,
    first_iteration: usize,
) -> Result<Value, String> {
    config.validate_v1()?;
    let bytes = serde_json::to_vec_pretty(config).map_err(err)?;
    let pin = write_pinned_v1(&root.join("cases").join(name).join("config.json"), &bytes)?;
    Ok(
        json!({"name": name, "config": pin, "output_directory": config.output_directory,
        "initial_source": config.initial_source, "first_iteration": first_iteration,
        "iterations": config.iterations.len()}),
    )
}

/// Writes the fixture (see the module documentation) and returns its
/// manifest, which is also `fixture.json`.
pub(super) fn write_fixture_v1(options: &FixtureOptionsV1) -> Result<Value, String> {
    options.validate_v1()?;
    if options.root.exists() {
        check(
            fs::read_dir(&options.root).map_err(err)?.next().is_none(),
            "fixture root must be absent or empty",
        )?;
    }
    fs::create_dir_all(&options.root).map_err(err)?;
    let root = options.root.canonicalize().map_err(err)?;
    let feature_identity = FRESH_FEATURE_IDENTITY_V4;
    let descriptor = write_synthetic_fresh_source_with_parameters_v1(
        &root.join("source"),
        feature_identity,
        |_| {},
    );
    let play_import = write_pinned_v1(
        &root.join("descriptor.json"),
        &serde_json::to_vec(&descriptor).map_err(err)?,
    )?;
    let fresh_source = ExpandedModelSourceV1 {
        play_import: play_import.clone(),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: feature_identity.feature_contract_digest.into(),
            expected_feature_encoding_digest: feature_identity.feature_encoding_digest.into(),
        },
        checkpoint: None,
        s_learner: None,
    };
    let work = |name: &str| root.join("work").join(name).join("run");
    let mut cases = vec![write_case_v1(
        &root,
        "fresh",
        &config_v1(
            options,
            fresh_source.clone(),
            iterations_v1(options, 0, options.updates),
            work("fresh"),
        ),
        0,
    )?];
    let mut warmup = Value::Null;
    if options.warmup_updates > 0 {
        let run = config_v1(
            options,
            fresh_source.clone(),
            iterations_v1(options, 0, options.warmup_updates),
            root.join("warmup").join("run"),
        );
        let result = run_native_expanded_training_v1(&run, None)?;
        check(
            result["complete"] == true,
            "fixture warm-up did not complete",
        )?;
        let warmed: ExpandedModelSourceV1 =
            serde_json::from_value(result["source"].clone()).map_err(err)?;
        let saved = warmed
            .checkpoint
            .as_ref()
            .ok_or("fixture warm-up published no checkpoint")?;
        let bytes = read_pin_bounded_shared_v1(saved, ARTIFACT_CAP)?;
        let retained = write_pinned_v1(&root.join("warm").join("checkpoint.json"), &bytes)?;
        let warm_source = ExpandedModelSourceV1 {
            checkpoint: Some(retained.clone()),
            ..fresh_source.clone()
        };
        cases.push(write_case_v1(
            &root,
            "warm",
            &config_v1(
                options,
                warm_source,
                iterations_v1(options, options.warmup_updates, options.updates),
                work("warm"),
            ),
            options.warmup_updates,
        )?);
        warmup = json!({"updates": options.warmup_updates, "run": run.output_directory,
            "checkpoint": saved, "retained_checkpoint": retained,
            "identity": result["actual_identity"]});
    }
    let pool = write_pool_cases_v1(options, &root, &fresh_source, &mut cases)?;
    let manifest = json!({
        "schema": FIXTURE_SCHEMA_V1,
        "generator": {
            "git_commit": env!("MTG_KERNEL_BUILD_GIT_HEAD"),
            "git_tree": env!("MTG_KERNEL_BUILD_GIT_TREE"),
            "git_clean": env!("MTG_KERNEL_BUILD_GIT_CLEAN"),
            "tracked_tree_sha256": env!("MTG_KERNEL_BUILD_TRACKED_TREE_SHA256"),
            "test": "native_expanded_training_run_v1::fixture_bench_v1::write_native_fixture_v1",
        },
        "options": {"seed": options.seed, "updates": options.updates, "games": options.games,
            "warmup_updates": options.warmup_updates,
            "collection_workers": options.collection_workers,
            "preparation_workers": options.preparation_workers,
            "pool_members": options.pool_members},
        "workload": {"learner": "net-8 FlatV4 synthetic fresh source", "decks": DECKS_V1,
            "learner_seat": 0, "starting_player": "game % 2", "opponent": "current",
            "seed": "seed + iteration * games + game", "loss_selection": GAE_V1,
            "learning_rate": LEARNING_RATE_V1, "value_coefficient": VALUE_COEFFICIENT_V1},
        "root": root,
        "descriptor": play_import,
        "cases": cases,
        "warmup": warmup,
        "pool": pool,
    });
    fs::write(
        root.join("fixture.json"),
        serde_json::to_vec_pretty(&manifest).map_err(err)?,
    )
    .map_err(err)?;
    Ok(manifest)
}

/// The draw's seed root: the fixture seed, in decimal.
fn pool_seed_root_v1(options: &FixtureOptionsV1) -> String {
    options.seed.to_string()
}

/// With `pool_members`, generates the largest pool's members under
/// `root/pool` and appends a `poolN` case for each size to `cases`; returns
/// the manifest's `pool` entry (`null` without pool cases).
fn write_pool_cases_v1(
    options: &FixtureOptionsV1,
    root: &Path,
    fresh_source: &ExpandedModelSourceV1,
    cases: &mut Vec<Value>,
) -> Result<Value, String> {
    use crate::expanded_deck_training_v1::pool_fixture_v1;
    let Some(&largest) = options.pool_members.last() else {
        return Ok(Value::Null);
    };
    let members = pool_fixture_v1::write_members_v1(&root.join("pool"), fresh_source, largest)?;
    let seed_root = pool_seed_root_v1(options);
    for &size in &options.pool_members {
        let name = format!("pool{size}");
        let mut config = config_v1(
            options,
            fresh_source.clone(),
            iterations_v1(options, 0, options.updates),
            root.join("work").join(&name).join("run"),
        );
        config.opponents = members[..size].to_vec();
        let draws =
            pool_fixture_v1::assign_draws_v1(&mut config.iterations, &config.opponents, &seed_root);
        let mut case = write_case_v1(root, &name, &config, 0)?;
        case["pool"] = json!({"members": size, "draws": draws});
        cases.push(case);
    }
    Ok(json!({
        "directory": root.join("pool"),
        "members": members.iter().map(|member| json!({"id": member.id,
            "checkpoint": member.source.checkpoint})).collect::<Vec<_>>(),
        "import": "the fresh source's descriptor, shared by every member",
        "draw": {"tag": pool_fixture_v1::DRAW_TAG_V1, "seed_root": seed_root,
            "rule": "slot j of update i plays member u64_be(sha256(json([tag, seed_root, i, j]))[0..8]) mod N, of the case's first N; the JSON is compact"},
    }))
}

/// The opt-in entry point: `MTG_NATIVE_FIXTURE_ROOT=/abs/root cargo test
/// --release --locked -p mtg-kernel --lib write_native_fixture_v1 --
/// --ignored --exact native_expanded_training_run_v1::fixture_bench_v1::write_native_fixture_v1`,
/// or `tools/native_fixture_bench/bench.py generate`.
#[test]
#[ignore = "writes the opt-in native benchmark fixture; run explicitly"]
fn write_native_fixture_v1() {
    let options = FixtureOptionsV1::from_env_v1().unwrap();
    let manifest = write_fixture_v1(&options).unwrap();
    println!(
        "NATIVE_FIXTURE_V1 {}",
        serde_json::to_string(&manifest).unwrap()
    );
}

fn temporary_root_v1(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "native-fixture-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn read_config_v1(case: &Value) -> NativeExpandedTrainingRunV1 {
    let pin: PinnedFileV1 = serde_json::from_value(case["config"].clone()).unwrap();
    serde_json::from_slice(&read_pin_bounded_shared_v1(&pin, SMALL_CAP).unwrap()).unwrap()
}

#[test]
fn the_fixture_schedule_is_explicit_and_every_case_writes_to_a_fixed_path() {
    let root = temporary_root_v1("schedule");
    let options = FixtureOptionsV1 {
        root: root.clone(),
        seed: 41,
        updates: 3,
        games: 2,
        warmup_updates: 0,
        collection_workers: 2,
        preparation_workers: 3,
        pool_members: vec![],
    };
    let manifest = write_fixture_v1(&options).unwrap();
    let root = root.canonicalize().unwrap();
    assert_eq!(manifest["schema"], FIXTURE_SCHEMA_V1);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(root.join("fixture.json")).unwrap()).unwrap(),
        manifest
    );
    assert_eq!(
        manifest["cases"].as_array().unwrap().len(),
        1,
        "no warm-up, no warm case"
    );
    assert_eq!(manifest["warmup"], Value::Null);
    let config = read_config_v1(&manifest["cases"][0]);
    assert_eq!(config.output_directory, root.join("work/fresh/run"));
    assert_eq!(config.initial_source.checkpoint, None);
    assert_eq!(
        (config.collection_workers, config.preparation_workers),
        (2, 3)
    );
    let seeds: Vec<Vec<(u64, u8)>> = config
        .iterations
        .iter()
        .map(|iteration| {
            iteration
                .episodes
                .iter()
                .map(|scheduled| {
                    assert!(matches!(
                        scheduled.opponent,
                        ExpandedOpponentAssignmentV1::Current
                    ));
                    let episode = &scheduled.episode;
                    assert_eq!(episode.learner_seat, 0);
                    assert_eq!(episode.registered[0].label, "Affinity");
                    assert_eq!(episode.registered[1].label, "Terror");
                    (episode.seed, episode.starting_player)
                })
                .collect()
        })
        .collect();
    assert_eq!(
        seeds,
        vec![
            vec![(41, 0), (42, 1)],
            vec![(43, 0), (44, 1)],
            vec![(45, 0), (46, 1)]
        ]
    );
    // The descriptor loads as a real fresh net-8 source.
    let (_, identity) =
        crate::expanded_deck_training_v1::load_expanded_inference_v1(&config.initial_source)
            .unwrap();
    assert_eq!(identity.adam_step, 0);
    // A second fixture never writes over a nonempty root.
    assert!(write_fixture_v1(&options)
        .unwrap_err()
        .contains("absent or empty"));
    fs::remove_dir_all(root).unwrap();
}

/// The warm-up runs the fresh schedule's first iterations in this process;
/// the warm case resumes its retained checkpoint with the next iterations.
/// The loads of that run are counted by caller role
/// (`loaded_source_profile_v1`): every lookup ends as a hit or a miss, each
/// update's readback is the one full load and the one kept entry, and every
/// other per-update load is a hit that copies a whole train state and policy.
#[test]
fn the_warm_case_resumes_the_warm_up_and_every_load_is_counted_by_role() {
    use crate::expanded_deck_training_v1::loaded_source_profile_v1::{
        counts_v1, reset_for_test_v1, LoadRoleV1,
    };
    let root = temporary_root_v1("warm");
    let options = FixtureOptionsV1 {
        root: root.clone(),
        seed: 2_026_091_601,
        updates: 1,
        games: 1,
        warmup_updates: 2,
        collection_workers: 1,
        preparation_workers: 1,
        pool_members: vec![],
    };
    reset_for_test_v1();
    let manifest = write_fixture_v1(&options).unwrap();
    // The whole table, shown if an assertion below fails.
    println!(
        "{}",
        crate::expanded_deck_training_v1::loaded_source_profile_v1::report_v1()
    );
    let all_roles = [
        LoadRoleV1::RunInitial,
        LoadRoleV1::RunFixedOpponent,
        LoadRoleV1::CollectLearner,
        LoadRoleV1::Opponent,
        LoadRoleV1::UpdateLearner,
        LoadRoleV1::UpdateReadback,
        LoadRoleV1::UpdateValidation,
        LoadRoleV1::Unattributed,
    ];
    // The warm-up's loads: a run start and two updates on this thread.
    let counted = all_roles.map(counts_v1);
    let count = |role: LoadRoleV1| counted[all_roles.iter().position(|r| *r == role).unwrap()];
    let updates = options.warmup_updates as u64;
    let initial = count(LoadRoleV1::RunInitial);
    assert_eq!(
        (
            initial.scoped_loads,
            initial.cache_lookups,
            initial.cache_misses,
            initial.cache_kept
        ),
        (1, 1, 1, 1),
        "the fresh descriptor loads in full once and is kept"
    );
    // Every state and policy of this net has one size: the kept fresh one's.
    let (state_bytes, policy_bytes) = (initial.kept_state_bytes, initial.kept_policy_bytes);
    let parameter_bytes = 4 * crate::native_policy_value_net_v1::PARAMETER_COUNT_V1 as u64;
    assert!(
        state_bytes >= 3 * parameter_bytes,
        "a state copy holds the parameters and both moments"
    );
    assert!(
        0 < policy_bytes && policy_bytes < parameter_bytes,
        "a policy copy holds its embedding table and shares its model"
    );
    let readback = count(LoadRoleV1::UpdateReadback);
    assert_eq!(
        (
            readback.scoped_loads,
            readback.cache_misses,
            readback.cache_kept,
            readback.cache_hits
        ),
        (updates, updates, updates, 0),
        "each update's readback is its new checkpoint's one full load"
    );
    assert_eq!(readback.kept_state_bytes, updates * state_bytes);
    assert_eq!(readback.kept_policy_bytes, updates * policy_bytes);
    for role in [
        LoadRoleV1::CollectLearner,
        LoadRoleV1::UpdateLearner,
        LoadRoleV1::UpdateValidation,
    ] {
        let counts = count(role);
        assert_eq!(
            (counts.scoped_loads, counts.cache_hits, counts.cache_misses),
            (updates, updates, 0),
            "{role:?}"
        );
        assert_eq!(counts.served_state_bytes, updates * state_bytes, "{role:?}");
        assert_eq!(
            counts.served_policy_bytes,
            updates * policy_bytes,
            "{role:?}"
        );
    }
    let opponent = count(LoadRoleV1::Opponent);
    assert!(
        opponent.cache_hits >= updates,
        "the current learner is its own opponent"
    );
    for (role, counts) in all_roles.iter().zip(&counted) {
        assert_eq!(
            counts.cache_lookups,
            counts.cache_hits + counts.cache_misses,
            "{role:?}: every lookup is a hit or a miss"
        );
    }
    assert_eq!(count(LoadRoleV1::Unattributed).cache_lookups, 0);
    assert_eq!(count(LoadRoleV1::RunFixedOpponent).scoped_loads, 0);

    let cases = manifest["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    let warm = read_config_v1(&cases[1]);
    let root = root.canonicalize().unwrap();
    assert_eq!(warm.output_directory, root.join("work/warm/run"));
    assert_eq!(cases[1]["first_iteration"], 2);
    assert_eq!(warm.iterations[0].episodes[0].episode.seed, 2_026_091_603);
    let retained = warm.initial_source.checkpoint.clone().unwrap();
    assert_eq!(retained.path, root.join("warm/checkpoint.json"));
    // The retained state is exactly the warm-up's last one.
    let (_, identity) =
        crate::expanded_deck_training_v1::load_expanded_inference_v1(&warm.initial_source).unwrap();
    assert_eq!(identity.adam_step, 2);
    assert_eq!(
        json!(identity.state_sha256),
        manifest["warmup"]["identity"]["state_sha256"]
    );
    fs::remove_dir_all(root).unwrap();
}

/// Pool cases (#599): generated members over the fresh import, every slot of
/// `poolN` playing the member of the first `N` that the published draw
/// names, the fresh case's episodes otherwise. A pool case runs, and the
/// opponent caches count its loads (`opponent_cache_diagnostics_v1`, this
/// thread's counters).
#[test]
fn pool_cases_play_their_drawn_members_and_count_the_opponent_loads() {
    use crate::expanded_deck_training_v1::opponent_cache_diagnostics_v1;
    use crate::expanded_deck_training_v1::pool_fixture_v1::{draw_v1, member_id_v1};
    let root = temporary_root_v1("pool");
    let options = FixtureOptionsV1 {
        root: root.clone(),
        seed: 2_026_091_601,
        updates: 1,
        games: 3,
        warmup_updates: 0,
        collection_workers: 1,
        preparation_workers: 1,
        pool_members: vec![1, 2],
    };
    let manifest = write_fixture_v1(&options).unwrap();
    let root = root.canonicalize().unwrap();
    let cases = manifest["cases"].as_array().unwrap();
    assert_eq!(
        cases.iter().map(|case| &case["name"]).collect::<Vec<_>>(),
        ["fresh", "pool1", "pool2"]
    );
    let members = manifest["pool"]["members"].as_array().unwrap();
    assert_eq!(members.len(), 2, "the largest pool's members, shared");
    for (index, member) in members.iter().enumerate() {
        assert_eq!(member["id"], member_id_v1(index));
        assert_eq!(
            member["checkpoint"]["path"],
            json!(root.join(format!("pool/member-{index:02}/checkpoint.json")))
        );
    }
    let seed_root = manifest["pool"]["draw"]["seed_root"].as_str().unwrap();
    assert_eq!(seed_root, "2026091601");
    let fresh = read_config_v1(&cases[0]);
    for (case, size) in cases[1..].iter().zip([1, 2]) {
        let config = read_config_v1(case);
        assert_eq!(
            config.output_directory,
            root.join(format!("work/pool{size}/run"))
        );
        assert_eq!(config.opponents.len(), size);
        for opponent in &config.opponents {
            assert_eq!(
                opponent.source.play_import,
                fresh.initial_source.play_import
            );
            assert!(opponent.source.checkpoint.is_some());
        }
        let mut draws = Vec::new();
        for (update, (iteration, fresh_iteration)) in
            config.iterations.iter().zip(&fresh.iterations).enumerate()
        {
            let mut ids = Vec::new();
            for (slot, (scheduled, fresh_scheduled)) in iteration
                .episodes
                .iter()
                .zip(&fresh_iteration.episodes)
                .enumerate()
            {
                let drawn = member_id_v1(draw_v1(seed_root, update, slot, size));
                assert!(matches!(
                    &scheduled.opponent,
                    ExpandedOpponentAssignmentV1::Fixed { id } if *id == drawn
                ));
                assert_eq!(
                    value(&scheduled.episode).unwrap(),
                    value(&fresh_scheduled.episode).unwrap()
                );
                ids.push(drawn);
            }
            draws.push(ids);
        }
        assert_eq!(case["pool"], json!({"members": size, "draws": draws}));
    }
    // pool2 runs; its draws name both members.
    let config = read_config_v1(&cases[2]);
    let drawn: BTreeSet<&str> = cases[2]["pool"]["draws"][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect();
    assert_eq!(drawn.len(), 2);
    let counters = opponent_cache_diagnostics_v1::counters_v1();
    let boundary = &counters.boundary;
    let collection = &counters.collection_opponent;
    let count = || {
        [
            boundary.checks.get(),
            boundary.sources.get(),
            boundary.cacheable_sources.get(),
            collection.misses_verified.get(),
            collection.hits.get(),
            counters.process_opponent.kept.get(),
        ]
    };
    let before = count();
    let result = run_native_expanded_training_v1(&config, None).unwrap();
    assert_eq!(result["complete"], true);
    let after = count();
    let complete_pin: PinnedFileV1 =
        serde_json::from_value(result["iterations"][0].clone()).unwrap();
    let complete = read_pin(&complete_pin).unwrap();
    let collection = read_pin(&parse_pin(&complete, "collection").unwrap()).unwrap();
    let trajectories = collection["trajectories"].as_array().unwrap();
    assert_eq!(trajectories.len(), options.games);
    for (slot, pin) in trajectories.iter().enumerate() {
        let pin: PinnedFileV1 = serde_json::from_value(pin.clone()).unwrap();
        let trajectory = read_pin_bounded(&pin, ARTIFACT_CAP).unwrap();
        let member = draw_v1(seed_root, 0, slot, 2);
        assert_eq!(
            trajectory["episode"]["opponent"]["checkpoint"],
            members[member]["checkpoint"]
        );
    }
    println!(
        "{}",
        json!(opponent_cache_diagnostics_v1::sections_v1()
            .into_iter()
            .map(|(name, section)| (name.to_string(), section))
            .collect::<serde_json::Map<String, Value>>())
    );
    // One collection boundary checked both drawn members, which the
    // process opponent cache may serve. The collector's cache missed each
    // once, through the process cache, which kept both, and served the third
    // game's repeat.
    assert_eq!(after[0] - before[0], 1);
    assert_eq!(after[1] - before[1], 2);
    assert_eq!(after[2] - before[2], 2);
    assert_eq!(after[3] - before[3], 2);
    assert!(after[4] - before[4] >= 1);
    assert!(after[5] - before[5] >= 2);
    fs::remove_dir_all(root).unwrap();
}
