#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::cube_draft_v1::{game_seed, CubePool, GameCaps, GameEnd};
use mtg_kernel::cube_learned_seat_v1::{BuildInputs, PickInputs, SeatForward};
use mtg_kernel::cube_pod_runner_v1::{attempt_seeds, pod_play_seed, watchdog_game_end};
use mtg_kernel::cube_seat_v1::{AttemptOutcome, AttemptSpec, GamePilot, SeatError};
use mtg_kernel::cube_training_driver_v1::{
    check_stop_ledger, prune_checkpoint, run_two_action_part3, tripwire_games_for_test,
    two_action_dropped_mean, CheckpointModel, CubeModelState, CubeUpdate, HeadUpdates,
    LineageSegment, OptimizerOutput, ParameterSetState, PilotPin, PilotProvider, PilotSchedule,
    PilotSelection, PilotSwitch, RunConfig, RunOutcome, Segment, TrainSets, TrainingDriver,
    TrainingDriverError, TripwireMode, TripwireRule, TwoActionGradient, UpdateBatch,
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

fn parameters(value: u32) -> ParameterSetState {
    ParameterSetState {
        parameters: vec![value],
        first_moment: vec![0],
        second_moment: vec![0],
        adam_step: 0,
    }
}

#[derive(Clone)]
struct TestModel {
    state: CubeModelState,
}

impl TestModel {
    fn new() -> Self {
        Self {
            state: CubeModelState {
                build: parameters(0),
                pick: parameters(100),
                value: parameters(200),
            },
        }
    }
}

impl SeatForward for TestModel {
    fn pick_and_value(&self, input: &PickInputs<'_>) -> Result<(Vec<f32>, f32), SeatError> {
        Ok((vec![0.0; input.pack.len()], 0.0))
    }

    fn count(&self, _pool: &[u16]) -> Result<[f32; 41], SeatError> {
        Ok([0.0; 41])
    }

    fn pointer(&self, _input: &BuildInputs<'_>, remaining: &[u16]) -> Result<Vec<f32>, SeatError> {
        Ok(vec![0.0; remaining.len()])
    }

    fn type_set(&self, _input: &BuildInputs<'_>, masks: &[u16]) -> Result<Vec<f32>, SeatError> {
        Ok(vec![0.0; masks.len()])
    }

    fn split(&self, _input: &BuildInputs<'_>, bits: &[u16]) -> Result<Vec<f32>, SeatError> {
        Ok(vec![0.0; bits.len()])
    }
}

impl CheckpointModel for TestModel {
    fn export_checkpoint(&self) -> CubeModelState {
        self.state.clone()
    }

    fn import_checkpoint(state: &CubeModelState) -> Result<Self, String> {
        Ok(Self {
            state: state.clone(),
        })
    }

    fn copy_build_trunk_to_pick(state: &mut CubeModelState) -> Result<(), String> {
        state.pick.parameters = state.build.parameters.clone();
        Ok(())
    }
}

#[derive(Default)]
struct TestUpdate;

impl CubeUpdate<TestModel> for TestUpdate {
    fn replay_and_commit(
        &mut self,
        model: &mut TestModel,
        batch: UpdateBatch<'_>,
        sets: TrainSets,
    ) -> Result<OptimizerOutput, String> {
        assert_eq!(batch.pods.len(), 16);
        assert!(!batch.league.is_empty());
        let mut state = model.export_checkpoint();
        for (enabled, set) in [
            (sets.build, &mut state.build),
            (sets.pick, &mut state.pick),
            (sets.value, &mut state.value),
        ] {
            if enabled {
                set.parameters[0] += 1;
                set.first_moment[0] += 2;
                set.second_moment[0] += 3;
                set.adam_step += 1;
            }
        }
        *model = TestModel { state };
        Ok(OptimizerOutput::default())
    }
}

struct NaturalPilot;

impl GamePilot for NaturalPilot {
    fn play(&self, spec: &AttemptSpec<'_>, _caps: &GameCaps) -> AttemptOutcome {
        AttemptOutcome {
            decisions: 3,
            physical_decisions: Some(4),
            policy_steps: Some(5),
            end: GameEnd::Natural {
                winner_seat: Some(spec.seats[(spec.action_seed % 2) as usize]),
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

struct CheckpointPilotProvider;

impl PilotProvider for CheckpointPilotProvider {
    fn load_checkpoint(
        &self,
        _pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        Ok(Arc::new(NaturalPilot))
    }
}

impl PilotProvider for NaturalPilot {
    fn load_checkpoint(
        &self,
        _pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        Ok(Arc::new(NaturalPilot))
    }
}

fn config(root: &Path, run: &str, segment: Segment, updates: u64) -> RunConfig {
    let mut config = RunConfig::shakedown(root, run, segment);
    config.update_range = [0, updates];
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
    struct Derivative;
    impl TwoActionGradient for Derivative {
        fn derivative(&self, outcome: [u8; 2]) -> f64 {
            let result = f64::from(outcome[0]) - f64::from(outcome[1]);
            let mut derivative = 0.0;
            for action in outcome {
                let seat_result = if action == outcome[0] {
                    result
                } else {
                    -result
                };
                derivative += -0.5 * seat_result * (f64::from(action) - 0.5);
            }
            derivative
        }
    }

    let temp = TempTestDir::new("two-action");
    let gradient = Derivative;
    let report = run_two_action_part3(&temp.join("half"), &gradient, false).expect("part 3");
    assert_eq!(report.published_batches, 63);
    assert_eq!(report.stopped_batches, 1);
    assert!((report.published_mean - -31.0 / 126.0).abs() < 1e-12);
    assert_ne!(report.published_mean, -0.25);
    assert!((two_action_dropped_mean(&gradient) - -3.0 / 14.0).abs() < 1e-12);
    assert_eq!(
        check_stop_ledger(&temp.join("half")),
        Err(TrainingDriverError::StoppedWithoutResolution)
    );

    let deterministic =
        run_two_action_part3(&temp.join("always"), &gradient, true).expect("deterministic halt");
    assert_eq!(deterministic.published_batches, 3);
    assert_eq!(deterministic.stopped_batches, 1);
    assert!((deterministic.published_mean - -1.0 / 6.0).abs() < 1e-12);
}

#[test]
fn head_updates_and_optimizer_state_cross_the_segment_boundary() {
    let temp = TempTestDir::new("boundary");
    let cube = pool();
    let mut builder = TrainingDriver::open(
        config(&temp.join("b1"), "b1", Segment::BuilderOnly, 1),
        TestModel::new(),
        TestUpdate,
    )
    .expect("open builder");
    assert_eq!(
        builder.run_next_update(&cube, &NaturalPilot),
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
        TestUpdate,
    )
    .expect("start pick phase");
    let initial = picks.checkpoint().expect("pick checkpoint");
    assert_eq!(initial.head_updates, heads);
    assert_eq!(
        initial.model.pick.parameters,
        builder_state.model.build.parameters
    );
    assert_eq!(initial.model.pick.adam_step, 0);
    assert_eq!(initial.model.value.adam_step, 0);
    assert_eq!(initial.model.pick.first_moment, vec![0]);
    assert_eq!(initial.model.value.second_moment, vec![0]);
    assert_eq!(initial.model.build.adam_step, 1);
    assert_eq!(picks.league().len(), builder.league().len());
    assert!(temp.join("k1").join(&picks.league()[0].path).exists());

    assert_eq!(
        picks.run_next_update(&cube, &NaturalPilot),
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
        TestModel::new(),
        TestUpdate,
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
    let error = TrainingDriver::open_with_lineage(config, TestModel::new(), TestUpdate, lineage)
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
    let mut driver = TrainingDriver::open(config, TestModel::new(), TestUpdate).expect("open");
    assert_eq!(
        driver.run_next_update(&pool(), &CheckpointPilotProvider),
        Ok(RunOutcome::Paused { update: 0 })
    );
    assert!(!temp.join("k2/pending/update-0000").exists());
    assert!(temp.join("k2/updates").exists());
    assert_eq!(driver.next_update(), 0);

    driver.set_gate_pin(pilot_pin()).expect("pin gate");
    assert_eq!(
        driver.run_next_update(&pool(), &CheckpointPilotProvider),
        Ok(RunOutcome::Published { update: 0 })
    );
}

#[test]
fn cap_stop_publishes_nothing_and_resume_refuses_without_resolution() {
    let temp = TempTestDir::new("stop");
    let root = temp.join("b1");
    let mut driver = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        TestModel::new(),
        TestUpdate,
    )
    .expect("open");
    assert_eq!(
        driver.run_next_update(&pool(), &AlwaysHaltPilot),
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
        TestModel::new(),
        TestUpdate,
    )
    .err()
    .expect("stopped run must refuse resume");
    assert_eq!(error, TrainingDriverError::StoppedWithoutResolution);
}

#[test]
fn kill_recovery_matches_uninterrupted_run_with_straddling_tripwire() {
    let temp = TempTestDir::new("resume");
    let cube = pool();
    let pod_seed = pod_play_seed("shakedown-pods", "b1", 0).expect("pod seed");
    let target_action_seed = attempt_seeds(game_seed(pod_seed, 0), 0).2;
    let pilot = OneHaltPilot { target_action_seed };
    let rules = TripwireMode::Full {
        rules: vec![TripwireRule::stop_kind_rate("decision_cap", 0.0)],
    };

    let uninterrupted_root = temp.join("direct");
    let mut direct_config = config(&uninterrupted_root, "b1", Segment::BuilderOnly, 2);
    direct_config.tripwire = rules.clone();
    let mut direct =
        TrainingDriver::open(direct_config, TestModel::new(), TestUpdate).expect("direct");
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
    let mut resumed = TrainingDriver::open(resumed_config.clone(), TestModel::new(), TestUpdate)
        .expect("resumed");
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

    let mut resumed = TrainingDriver::open(resumed_config, TestModel::new(), TestUpdate)
        .expect("recover committed update");
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
    assert_eq!(
        directory_bytes(&uninterrupted_root),
        directory_bytes(&resumed_root)
    );

    fs::remove_file(resumed_root.join("stops/stop-0000.json")).expect("remove ledger copy");
    let before = directory_bytes(&resumed_root);
    let error = TrainingDriver::open(
        config(&resumed_root, "b1", Segment::BuilderOnly, 2),
        TestModel::new(),
        TestUpdate,
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
    let mut one =
        TrainingDriver::open(one_config, TestModel::new(), TestUpdate).expect("one worker");
    assert_eq!(
        one.run_next_update(&cube, &NaturalPilot),
        Ok(RunOutcome::Published { update: 0 })
    );
    let mut twelve_config = config(&twelve_root, "b1", Segment::BuilderOnly, 1);
    twelve_config.workers = 12;
    let mut twelve =
        TrainingDriver::open(twelve_config, TestModel::new(), TestUpdate).expect("twelve workers");
    assert_eq!(
        twelve.run_next_update(&cube, &NaturalPilot),
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
    let pilot = FirstWatchdogPilot {
        fired: AtomicBool::new(false),
    };
    let mut rerun = TrainingDriver::open(
        config(&rerun_root, "b1", Segment::BuilderOnly, 1),
        TestModel::new(),
        TestUpdate,
    )
    .expect("rerun driver");
    assert_eq!(
        rerun.run_next_update(&cube, &pilot),
        Ok(RunOutcome::Published { update: 0 })
    );
    assert!(pilot.fired.load(Ordering::SeqCst));
    assert!(!rerun_root.join("stops").exists());

    let mut direct = TrainingDriver::open(
        config(&direct_root, "b1", Segment::BuilderOnly, 1),
        TestModel::new(),
        TestUpdate,
    )
    .expect("direct driver");
    assert_eq!(
        direct.run_next_update(&cube, &NaturalPilot),
        Ok(RunOutcome::Published { update: 0 })
    );
    assert_eq!(directory_bytes(&rerun_root), directory_bytes(&direct_root));
}

#[test]
fn repeat_watchdog_on_same_game_number_in_another_pod_is_not_a_hang() {
    let temp = TempTestDir::new("watchdog-two-pods");
    let root = temp.join("b1");
    let pilot = RepeatWatchdogPilot {
        calls: AtomicUsize::new(0),
        second_call: 70,
    };
    let mut driver = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        TestModel::new(),
        TestUpdate,
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
        TestModel::new(),
        TestUpdate,
    )
    .err()
    .expect("unfinished batch stops the run");
    assert_eq!(error, TrainingDriverError::StoppedWithoutResolution);
    assert!(!root.join("updates/update-0000").exists());
    assert!(!root.join("pending/update-0000").exists());
    let stop = fs::read_to_string(root.join("stops/stop-0000.json")).expect("stop record");
    assert!(stop.contains("\"kind\": \"process_death\""));
}
