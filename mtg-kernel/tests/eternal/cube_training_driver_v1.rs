#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::cube_draft_build_v1::model::{
    training::fixture::{CAPPED_MEAN, DETERMINISTIC_HALT_MEAN, DROP_MEAN, UNLIMITED_MEAN},
    ModelCardRowsV1,
};
use mtg_kernel::cube_draft_v1::{basic_land_ids, game_seed, CubePool, GameCaps, GameEnd};
use mtg_kernel::cube_pod_runner_v1::{attempt_seeds, pod_play_seed, watchdog_game_end};
use mtg_kernel::cube_seat_v1::{AttemptOutcome, AttemptSpec, GamePilot};
use mtg_kernel::cube_training_driver_v1::{
    check_stop_ledger, prune_checkpoint, run_two_action_part3, tripwire_games_for_test,
    two_action_dropped_mean, LineageSegment, PilotPin, PilotProvider, PilotSchedule,
    PilotSelection, PilotSwitch, RunConfig, RunOutcome, Segment, TrainingDriver,
    TrainingDriverError, TripwireMode, TripwireRule,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

static TEST_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

struct TempTestDir {
    path: PathBuf,
}

impl TempTestDir {
    fn new(label: &str) -> Self {
        let sequence = TEST_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/cube-d6-tests")
            .join(format!("{label}-{}-{sequence}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create test root");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn pool() -> CubePool {
    CubePool::from_json_texts(
        include_str!("../../../data/cube_v1/mtgo_vintage_cube_20261007.json"),
        include_str!("../../../data/cube_v1/mtgo_vintage_cube_20261007_gaps.json"),
        Some(r#"{"cards": []}"#),
    )
    .expect("cube pool")
}

fn model_rows() -> Arc<ModelCardRowsV1> {
    let mut ids = pool().ids().to_vec();
    ids.extend(basic_land_ids().expect("basic ids"));
    ids.sort_unstable();
    ids.dedup();
    let len = ids.len();
    Arc::new(
        ModelCardRowsV1::with_registry_basics_v1(ids, 1, vec![0.0; len], 1, vec![0.0; len])
            .expect("fixture card rows"),
    )
}

struct FixturePilot<P>(Arc<P>);

impl<P: GamePilot + Sync + 'static> PilotProvider for FixturePilot<P> {
    fn load_checkpoint(
        &self,
        _pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        Ok(self.0.clone())
    }
}

struct NaturalPilot;

impl GamePilot for NaturalPilot {
    fn play(&self, spec: &AttemptSpec<'_>, _caps: &GameCaps) -> AttemptOutcome {
        let deck_key = spec.decks[0]
            .card_ids()
            .into_iter()
            .fold(spec.action_seed, |key, id| {
                key.rotate_left(11)
                    .wrapping_mul(0x9e37_79b9)
                    .wrapping_add(u64::from(id))
            });
        AttemptOutcome {
            decisions: 3,
            physical_decisions: Some(4),
            policy_steps: Some(5),
            end: GameEnd::Natural {
                winner_seat: Some(spec.seats[((deck_key ^ (deck_key >> 32)) % 2) as usize]),
                reason: "fixture".to_owned(),
            },
            last_action_source: None,
        }
    }
}

struct AlwaysHaltPilot;

impl GamePilot for AlwaysHaltPilot {
    fn play(&self, _spec: &AttemptSpec<'_>, _caps: &GameCaps) -> AttemptOutcome {
        AttemptOutcome {
            decisions: 1,
            physical_decisions: Some(1),
            policy_steps: Some(1),
            end: GameEnd::DecisionCap,
            last_action_source: None,
        }
    }
}

struct FirstWatchdogPilot {
    fired: AtomicBool,
}

impl GamePilot for FirstWatchdogPilot {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        if !self.fired.swap(true, Ordering::SeqCst) {
            return AttemptOutcome {
                decisions: 0,
                physical_decisions: Some(0),
                policy_steps: Some(0),
                end: watchdog_game_end(),
                last_action_source: None,
            };
        }
        NaturalPilot.play(spec, caps)
    }
}

struct RepeatWatchdogPilot {
    calls: AtomicUsize,
    second_call: usize,
}

impl GamePilot for RepeatWatchdogPilot {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 || call == self.second_call {
            return AttemptOutcome {
                decisions: 0,
                physical_decisions: Some(0),
                policy_steps: Some(0),
                end: watchdog_game_end(),
                last_action_source: None,
            };
        }
        NaturalPilot.play(spec, caps)
    }
}

struct OneHaltPilot {
    target_action_seed: u64,
}

impl GamePilot for OneHaltPilot {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        if spec.action_seed == self.target_action_seed {
            return AlwaysHaltPilot.play(spec, caps);
        }
        NaturalPilot.play(spec, caps)
    }
}

fn config(root: &Path, run: &str, segment: Segment, updates: u64) -> RunConfig {
    let mut config = RunConfig::shakedown(root, run, segment);
    config.update_range = [0, updates];
    config.label = "test".to_owned();
    config.namespace = "test".to_owned();
    config.pilot.initial = PilotSelection::Checkpoint { pin: pilot_pin() };
    config
}

fn pilot_pin() -> PilotPin {
    PilotPin {
        checkpoint_sha256: "11".repeat(32),
        after_state_sha256: "22".repeat(32),
        card_database_sha256: "33".repeat(32),
        table_sha256: "44".repeat(32),
        training_math: "portable-math-v1".to_owned(),
        source_binary_sha256: "55".repeat(32),
        decoding: "uniform-index-splitmix64-v1".to_owned(),
    }
}

fn directory_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, output: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let mut entries: Vec<_> = fs::read_dir(directory)
            .expect("read directory")
            .map(|entry| entry.expect("directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                visit(root, &path, output);
            } else {
                output.insert(
                    path.strip_prefix(root)
                        .expect("relative path")
                        .to_path_buf(),
                    fs::read(&path).expect("read file"),
                );
            }
        }
    }
    let mut output = BTreeMap::new();
    visit(root, root, &mut output);
    output
}

#[test]
fn two_action_part3_uses_real_replay_cap_and_stop() {
    let temp = TempTestDir::new("two-action");
    let report = run_two_action_part3(&temp.join("half"), false).expect("part 3");
    assert_eq!(report.published_batches, 63);
    assert_eq!(report.stopped_batches, 1);
    assert!((report.published_mean - CAPPED_MEAN).abs() < 1e-12);
    assert_ne!(report.published_mean, UNLIMITED_MEAN);
    assert!((two_action_dropped_mean() - DROP_MEAN).abs() < 1e-12);
    assert_eq!(
        check_stop_ledger(&temp.join("half")),
        Err(TrainingDriverError::StoppedWithoutResolution)
    );

    let deterministic =
        run_two_action_part3(&temp.join("always"), true).expect("deterministic halt");
    assert_eq!(deterministic.published_batches, 3);
    assert_eq!(deterministic.stopped_batches, 1);
    assert!((deterministic.published_mean - DETERMINISTIC_HALT_MEAN).abs() < 1e-12);
    for root in [temp.join("half"), temp.join("always")] {
        assert!(!root.join("updates").exists());
        let stop: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("stops/stop-0000.json")).expect("fixture stop"),
        )
        .expect("fixture stop JSON");
        assert_eq!(stop["kind"], "retry_exhausted");
        assert_eq!(stop["attempts"].as_array().expect("attempts").len(), 4);
    }
}

#[test]
fn head_updates_and_optimizer_state_cross_the_segment_boundary() {
    let temp = TempTestDir::new("boundary");
    let cube = pool();
    let mut builder = TrainingDriver::open(
        config(&temp.join("b1"), "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .expect("open builder");
    assert_eq!(
        builder.run_next_update(&cube, &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Published { update: 0 })
    );
    let heads = builder.head_updates();
    assert_eq!(heads.pick, 0);
    assert_eq!(heads.value, 0);
    assert_eq!(
        (heads.count, heads.pointer, heads.type_set, heads.split),
        (1, 1, 1, 1)
    );
    let builder_state = builder.checkpoint().expect("builder checkpoint");

    let mut picks = TrainingDriver::start_pick_phase(
        &builder,
        config(&temp.join("k1"), "k1", Segment::PickPhase, 1),
    )
    .expect("start pick phase");
    let initial = picks.checkpoint().expect("pick checkpoint");
    assert_eq!(initial.head_updates, heads);
    assert_eq!(initial.model.build, builder_state.model.build);
    assert_eq!(
        initial.model.value.parameters,
        builder_state.model.value.parameters
    );
    assert_ne!(
        initial.model.pick.parameters,
        builder_state.model.pick.parameters
    );
    assert_eq!(initial.model.pick.adam_step, 0);
    assert!(builder_state
        .model
        .build
        .first_moment
        .iter()
        .any(|&value| value != 0));
    assert_eq!(initial.model.value.adam_step, 0);
    assert!(initial
        .model
        .pick
        .first_moment
        .iter()
        .all(|&value| value == 0));
    assert!(initial
        .model
        .value
        .second_moment
        .iter()
        .all(|&value| value == 0));
    assert_eq!(initial.model.build.adam_step, 1);
    assert_eq!(picks.league().len(), builder.league().len());
    assert!(temp.join("k1").join(&picks.league()[0].path).exists());

    assert_eq!(
        picks.run_next_update(&cube, &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Published { update: 0 })
    );
    let heads = picks.head_updates();
    assert_eq!((heads.pick, heads.value), (1, 1));
    assert_eq!(
        (heads.count, heads.pointer, heads.type_set, heads.split),
        (2, 2, 2, 2)
    );
    let final_state = picks.checkpoint().expect("final checkpoint");
    assert_eq!(final_state.model.build.adam_step, 2);
    assert_eq!(final_state.model.pick.adam_step, 1);
    assert_eq!(final_state.model.value.adam_step, 1);

    let checkpoint_path = temp.join("k1/updates/update-0000/checkpoint.json");
    let json = fs::read_to_string(checkpoint_path).expect("checkpoint json");
    let head_position = json.rfind("\"head_updates\"").expect("last header key");
    assert!(head_position > json.rfind("\"state_sha256\"").expect("state hash"));
    let tail = &json[head_position..];
    let keys = ["pick", "value", "count", "pointer", "type_set", "split"];
    let positions = keys.map(|key| tail.find(&format!("\"{key}\"")).expect("head key"));
    assert!(positions.windows(2).all(|window| window[0] < window[1]));
}

#[test]
fn pinned_league_snapshot_cannot_be_pruned() {
    let temp = TempTestDir::new("pinned");
    let driver = TrainingDriver::open(
        config(&temp.join("b1"), "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .expect("open");
    let snapshot = temp.join("b1").join(&driver.league()[0].path);
    assert!(prune_checkpoint(&snapshot).is_err());
    assert!(snapshot.exists());
}

#[test]
fn uniform_pilot_lineage_is_refused_outside_shakedown() {
    let temp = TempTestDir::new("lineage");
    let mut config = config(&temp.join("m5"), "b2", Segment::PickPhase, 1);
    config.label = "training".to_owned();
    config.namespace = "m5-train".to_owned();
    config.pilot = PilotSchedule {
        initial: PilotSelection::Checkpoint { pin: pilot_pin() },
        switch: None,
    };
    let lineage = vec![LineageSegment {
        segment: Segment::BuilderOnly,
        run: "b1".to_owned(),
        pilots: vec![mtg_kernel::cube_pod_runner_v1::PilotKind::Uniform],
    }];
    let error = TrainingDriver::open_with_lineage(config, model_rows(), 1234, lineage)
        .err()
        .expect("uniform lineage must fail");
    assert!(error.to_string().contains("uniform-pilot lineage"));
}

#[test]
fn missing_gate_pin_pauses_before_touching_a_batch() {
    let temp = TempTestDir::new("gate");
    let mut config = config(&temp.join("k2"), "k2", Segment::PickPhase, 1);
    config.label = "training".to_owned();
    config.namespace = "m5-train".to_owned();
    config.pilot = PilotSchedule {
        initial: PilotSelection::Checkpoint { pin: pilot_pin() },
        switch: Some(PilotSwitch {
            update: 0,
            gate: None,
        }),
    };
    let mut driver = TrainingDriver::open(config, model_rows(), 1234).expect("open");
    assert_eq!(
        driver.run_next_update(&pool(), &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Paused { update: 0 })
    );
    assert!(!temp.join("k2/pending/update-0000").exists());
    assert!(temp.join("k2/updates").exists());
    assert_eq!(driver.next_update(), 0);

    driver.set_gate_pin(pilot_pin()).expect("pin gate");
    assert_eq!(
        driver.run_next_update(&pool(), &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Published { update: 0 })
    );
}

#[test]
fn cap_stop_publishes_nothing_and_resume_refuses_without_resolution() {
    let temp = TempTestDir::new("stop");
    let root = temp.join("b1");
    let mut driver = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .expect("open");
    assert_eq!(
        driver.run_next_update(&pool(), &FixturePilot(Arc::new(AlwaysHaltPilot))),
        Ok(RunOutcome::Stopped {
            update: 0,
            kind: "retry_exhausted".to_owned(),
        })
    );
    assert!(!root.join("updates/update-0000").exists());
    assert!(root.join("stops/stop-0000.json").exists());
    assert!(!root.join("pending/update-0000").exists());
    let error = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .err()
    .expect("stopped run must refuse resume");
    assert_eq!(error, TrainingDriverError::StoppedWithoutResolution);
    fs::write(root.join("stops/stop-0000.resolution.json"), b"{}").expect("fixture resolution");
    assert_eq!(
        check_stop_ledger(&root),
        Err(TrainingDriverError::StoppedRequiresRestart)
    );
}

#[test]
fn kill_recovery_matches_uninterrupted_run_with_straddling_tripwire() {
    let temp = TempTestDir::new("resume");
    let cube = pool();
    let pod_seed = pod_play_seed("test", "b1", 0).expect("pod seed");
    let target_action_seed = attempt_seeds(game_seed(pod_seed, 0), 0).2;
    let pilot = FixturePilot(Arc::new(OneHaltPilot { target_action_seed }));
    let rules = TripwireMode::Full {
        rules: vec![TripwireRule::stop_kind_rate("decision_cap", 0.0)],
    };

    let uninterrupted_root = temp.join("direct");
    let mut direct_config = config(&uninterrupted_root, "b1", Segment::BuilderOnly, 2);
    direct_config.tripwire = rules.clone();
    let mut direct = TrainingDriver::open(direct_config, model_rows(), 1234).expect("direct");
    assert_eq!(
        direct.run_next_update(&cube, &pilot),
        Ok(RunOutcome::Published { update: 0 })
    );
    assert_eq!(
        direct.run_next_update(&cube, &pilot),
        Ok(RunOutcome::Stopped {
            update: 1,
            kind: "tripwire".to_owned(),
        })
    );

    let resumed_root = temp.join("resumed");
    let mut resumed_config = config(&resumed_root, "b1", Segment::BuilderOnly, 2);
    resumed_config.tripwire = rules.clone();
    let mut resumed =
        TrainingDriver::open(resumed_config.clone(), model_rows(), 1234).expect("resumed");
    assert_eq!(
        resumed.run_next_update(&cube, &pilot),
        Ok(RunOutcome::Published { update: 0 })
    );
    drop(resumed);
    fs::rename(
        resumed_root.join("updates/update-0000"),
        resumed_root.join("pending/update-0000"),
    )
    .expect("simulate committed pending update");

    let mut resumed =
        TrainingDriver::open(resumed_config, model_rows(), 1234).expect("recover committed update");
    assert_eq!(resumed.next_update(), 1);
    assert_eq!(
        resumed.run_next_update(&cube, &pilot),
        Ok(RunOutcome::Stopped {
            update: 1,
            kind: "tripwire".to_owned(),
        })
    );
    assert_eq!(
        tripwire_games_for_test(&resumed_root, &rules).expect("games"),
        2_240
    );
    let stop: serde_json::Value = serde_json::from_slice(
        &fs::read(resumed_root.join("stops/stop-0000.json")).expect("window stop"),
    )
    .expect("window stop JSON");
    assert_eq!(stop["firing"]["denominator"], 2_240);
    assert_eq!(stop["firing"]["games"], 1);
    assert_eq!(
        directory_bytes(&uninterrupted_root),
        directory_bytes(&resumed_root)
    );

    fs::remove_file(resumed_root.join("stops/stop-0000.json")).expect("remove ledger copy");
    let before = directory_bytes(&uninterrupted_root);
    let error = TrainingDriver::open(
        config(&resumed_root, "b1", Segment::BuilderOnly, 2),
        model_rows(),
        1234,
    )
    .err()
    .expect("recovery restores the stop then refuses");
    assert_eq!(error, TrainingDriverError::StoppedWithoutResolution);
    assert_eq!(before, directory_bytes(&resumed_root));
}

#[test]
fn worker_count_does_not_change_pods_or_checkpoint() {
    let temp = TempTestDir::new("workers");
    let cube = pool();
    let one_root = temp.join("one");
    let twelve_root = temp.join("twelve");
    let mut one_config = config(&one_root, "b1", Segment::BuilderOnly, 1);
    one_config.workers = 1;
    let mut one = TrainingDriver::open(one_config, model_rows(), 1234).expect("one worker");
    assert_eq!(
        one.run_next_update(&cube, &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Published { update: 0 })
    );
    let mut twelve_config = config(&twelve_root, "b1", Segment::BuilderOnly, 1);
    twelve_config.workers = 12;
    let mut twelve =
        TrainingDriver::open(twelve_config, model_rows(), 1234).expect("twelve workers");
    assert_eq!(
        twelve.run_next_update(&cube, &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Published { update: 0 })
    );
    for name in ["pods.json", "checkpoint.json"] {
        assert_eq!(
            fs::read(one_root.join("updates/update-0000").join(name)).expect("one output"),
            fs::read(twelve_root.join("updates/update-0000").join(name)).expect("twelve output"),
            "{name} differs by worker count"
        );
    }
}

#[test]
fn first_watchdog_expiry_reruns_the_same_batch_once() {
    let temp = TempTestDir::new("watchdog");
    let cube = pool();
    let rerun_root = temp.join("rerun");
    let direct_root = temp.join("direct");
    let pilot = FixturePilot(Arc::new(FirstWatchdogPilot {
        fired: AtomicBool::new(false),
    }));
    let mut rerun = TrainingDriver::open(
        config(&rerun_root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .expect("rerun driver");
    assert_eq!(
        rerun.run_next_update(&cube, &pilot),
        Ok(RunOutcome::Published { update: 0 })
    );
    assert!(pilot.0.fired.load(Ordering::SeqCst));
    assert!(!rerun_root.join("stops").exists());

    let mut direct = TrainingDriver::open(
        config(&direct_root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .expect("direct driver");
    assert_eq!(
        direct.run_next_update(&cube, &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Published { update: 0 })
    );
    assert_eq!(directory_bytes(&rerun_root), directory_bytes(&direct_root));
}

#[test]
fn repeat_watchdog_on_same_game_number_in_another_pod_is_not_a_hang() {
    let temp = TempTestDir::new("watchdog-two-pods");
    let root = temp.join("b1");
    let pilot = FixturePilot(Arc::new(RepeatWatchdogPilot {
        calls: AtomicUsize::new(0),
        second_call: 71,
    }));
    let mut driver = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .expect("driver");
    assert_eq!(
        driver.run_next_update(&pool(), &pilot),
        Ok(RunOutcome::Stopped {
            update: 0,
            kind: "watchdog_stop".to_owned(),
        })
    );
    let stop: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("stops/stop-0000.json")).expect("stop record"))
            .expect("stop json");
    assert_eq!(stop["hang"], false);
    assert_eq!(stop["first_watchdog_pod"], 0);
    assert_eq!(stop["second_watchdog_pod"], 1);
    assert_eq!(stop["first_watchdog_game"], 0);
    assert_eq!(stop["second_watchdog_game"], 0);
}

#[test]
fn invalid_pending_batch_is_abandoned_not_published() {
    let temp = TempTestDir::new("abandoned");
    let root = temp.join("b1");
    fs::create_dir_all(root.join("pending/update-0000")).expect("pending batch");
    fs::write(root.join("pending/update-0000/partial.txt"), b"lost").expect("partial write");
    let error = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .err()
    .expect("unfinished batch stops the run");
    assert_eq!(error, TrainingDriverError::StoppedWithoutResolution);
    assert!(!root.join("updates/update-0000").exists());
    assert!(!root.join("pending/update-0000").exists());
    let stop = fs::read_to_string(root.join("stops/stop-0000.json")).expect("stop record");
    assert!(stop.contains("\"kind\": \"process_death\""));
}
