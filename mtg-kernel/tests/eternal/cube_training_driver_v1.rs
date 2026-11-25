#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use mtg_kernel::cube_draft_build_v1::card_index::{CardIndexV1, CubeTableSourceV1};
use mtg_kernel::cube_draft_build_v1::model::{
    training::fixture::{CAPPED_MEAN, DETERMINISTIC_HALT_MEAN, DROP_MEAN, UNLIMITED_MEAN},
    ModelCardRowsV1, ModelV1,
};
use mtg_kernel::cube_draft_v1::{basic_land_ids, game_seed, CubePool, GameCaps, GameEnd};
use mtg_kernel::cube_pod_runner_v1::{attempt_seeds, pod_play_seed, watchdog_game_end};
use mtg_kernel::cube_read_plan_v1::sha256_hex;
use mtg_kernel::cube_seat_v1::{AttemptOutcome, AttemptSpec, GamePilot};
use mtg_kernel::cube_training_driver_v1::fixtures::{
    run_two_action_part3, two_action_dropped_mean,
};
use mtg_kernel::cube_training_driver_v1::{
    check_stop_ledger, load_checkpoint_read_only_v1, prune_checkpoint, tripwire_games_for_test,
    CensusBaselinePin, LineageSegment, PilotPin, PilotProvider, PilotSchedule, PilotSelection,
    PilotSwitch, ResumeAuthorization, RunConfig, RunOutcome, Segment, TrainingDriver,
    TrainingDriverError, TripwireMode,
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

struct StraddlingTripwirePilot {
    halt_action_seeds: Vec<u64>,
    watchdog_action_seed: u64,
    watchdog: FirstWatchdogPilot,
}

impl GamePilot for StraddlingTripwirePilot {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        if spec.action_seed == self.watchdog_action_seed {
            return self.watchdog.play(spec, caps);
        }
        if self.halt_action_seeds.contains(&spec.action_seed) {
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

fn directory_bytes(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, directory: &Path, output: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        let mut entries: Vec<_> = fs::read_dir(directory)
            .expect("read directory")
            .map(|entry| entry.expect("directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            let key = path
                .strip_prefix(root)
                .expect("relative path")
                .to_path_buf();
            if path.is_dir() {
                output.insert(key, None);
                visit(root, &path, output);
            } else {
                output.insert(key, Some(fs::read(&path).expect("read file")));
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
    use mtg_kernel::cube_draft_build_v1::model::ParameterSetV1;
    let rows = model_rows();
    let layout =
        ModelV1::initialise_v1(rows.rows(), rows.table_width(), rows.dense_width(), 1234).unwrap();
    let ranges = |set| {
        let mut offset = 0;
        layout
            .layout()
            .iter()
            .filter(|spec| spec.set == set)
            .map(|spec| {
                let start = offset;
                offset += spec.element_count();
                (spec.name.as_str(), start..offset)
            })
            .collect::<BTreeMap<_, _>>()
    };
    let build_ranges = ranges(ParameterSetV1::Build);
    let pick_ranges = ranges(ParameterSetV1::Pick);
    for (name, range) in pick_ranges {
        let pick_bits = &initial.model.pick.parameters[range.clone()];
        if let Some(trunk_suffix) = name.strip_prefix("pick_trunk.") {
            let source_range = build_ranges[format!("build_trunk.{trunk_suffix}").as_str()].clone();
            assert_eq!(
                pick_bits, &builder_state.model.build.parameters[source_range],
                "{name}"
            );
        } else {
            assert_eq!(
                pick_bits, &builder_state.model.pick.parameters[range],
                "{name}"
            );
        }
    }
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
    let receipt = read_value(&temp.join("k1/updates/update-0000/update.json"));
    let statistics = &receipt["optimizer"]["statistics"];
    assert_eq!(statistics["value_target"]["count"], 4032);
    assert_eq!(statistics["value_prediction"]["count"], 4032);
    assert_eq!(statistics["pick_advantage"]["count"], 4032);
    assert_eq!(statistics["entropy"]["pick"]["count"], 4032);
    assert!(receipt["optimizer"]["gradient_norms"]["value"].is_number());
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
    assert_eq!(
        prune_checkpoint(&snapshot),
        Err(TrainingDriverError::PinnedCheckpoint)
    );
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
    let mut config = config(&temp.join("k2"), "k2", Segment::BuilderOnly, 1);
    config.label = "training".to_owned();
    config.namespace = "m5-train".to_owned();
    config.pilot = PilotSchedule {
        initial: PilotSelection::Checkpoint { pin: pilot_pin() },
        switch: Some(PilotSwitch {
            update: 0,
            gate: None,
        }),
    };
    let mut driver = TrainingDriver::open(config.clone(), model_rows(), 1234).expect("open");
    assert_eq!(
        driver.run_next_update(&pool(), &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Paused { update: 0 })
    );
    assert!(!temp.join("k2/pending/update-0000").exists());
    assert!(temp.join("k2/updates").exists());
    assert_eq!(driver.next_update(), 0);

    let run_path = temp.join("k2/run.json");
    let old = fs::read(&run_path).unwrap();
    let old_link = temp.join("k2/old-run.json");
    fs::hard_link(&run_path, &old_link).unwrap();
    driver.set_gate_pin(pilot_pin()).expect("pin gate");
    assert_eq!(fs::read(&old_link).unwrap(), old);
    assert_ne!(fs::read(&run_path).unwrap(), old);
    // Kill after the immutable pin was published but before replacing run.json.
    drop(driver);
    fs::write(&run_path, &old).unwrap();
    config.pilot.switch.as_mut().unwrap().gate = Some(pilot_pin());
    let mut driver = TrainingDriver::open(config.clone(), model_rows(), 1234).unwrap();
    assert_eq!(
        read_value(&run_path)["pilot"]["switch"]["gate"]["checkpoint_sha256"],
        pilot_pin().checkpoint_sha256
    );
    assert_eq!(
        driver.run_next_update(&pool(), &FixturePilot(Arc::new(NaturalPilot))),
        Ok(RunOutcome::Published { update: 0 })
    );
    let mut resume_only = config.clone();
    resume_only.root = temp.join("resume-only");
    resume_only.pilot.switch.as_mut().unwrap().gate = None;
    TrainingDriver::open(resume_only.clone(), model_rows(), 1234).unwrap();
    resume_only.pilot.switch.as_mut().unwrap().gate = Some(pilot_pin());
    TrainingDriver::open(resume_only.clone(), model_rows(), 1234).unwrap();
    assert_eq!(
        read_value(&resume_only.root.join("run.json"))["pilot"]["switch"]["gate"]
            ["checkpoint_sha256"],
        pilot_pin().checkpoint_sha256
    );
    let mut wrong = pilot_pin();
    wrong.checkpoint_sha256 = "99".repeat(32);
    config.pilot.switch.as_mut().unwrap().gate = Some(wrong);
    let error = TrainingDriver::open(config, model_rows(), 1234)
        .err()
        .unwrap();
    assert!(error.to_string().contains("fixed gate pin"));
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
    use mtg_kernel::cube_training_driver_v1::PODS_PER_UPDATE;
    let halt_action_seeds: Vec<_> = (0..4)
        .map(|group| attempt_seeds(game_seed(pod_seed, group), 0).2)
        .collect();
    let watchdog_seed = pod_play_seed("test", "b1", PODS_PER_UPDATE).unwrap();
    let new_pilot = || {
        FixturePilot(Arc::new(StraddlingTripwirePilot {
            halt_action_seeds: halt_action_seeds.clone(),
            watchdog_action_seed: attempt_seeds(game_seed(watchdog_seed, 0), 0).2,
            watchdog: FirstWatchdogPilot {
                fired: AtomicBool::new(false),
            },
        }))
    };
    let pilot = new_pilot();
    let summary_path = temp.join("summary.json");
    let bytes = serde_json::to_vec(&serde_json::json!({
        "games": 2000, "completed": 1999, "halted": 1,
        "stop_kinds": [{"stop_kind": "driver_decision_cap", "games": 1}],
        "cards": [], "order_errors": [], "stop_kind_mismatches": []
    }))
    .unwrap();
    fs::write(&summary_path, &bytes).unwrap();
    let baseline = CensusBaselinePin {
        run: "census-b".into(),
        summary_sha256: sha256_hex(&bytes),
        manifest_sha256: "11".repeat(32),
        build_sha256: "22".repeat(32),
        fence_sha256: "33".repeat(32),
        authorization_sha256: "44".repeat(32),
    };
    let bind = |config: &mut RunConfig| {
        config.pins.binary = Some(baseline.build_sha256.clone());
        config.pins.fence_record = Some(baseline.fence_sha256.clone());
        config.tripwire = TripwireMode::Full {
            baseline: baseline.clone(),
        };
        config.tripwire_summary = Some(summary_path.clone());
    };

    let uninterrupted_root = temp.join("direct");
    let mut direct_config = config(&uninterrupted_root, "b1", Segment::BuilderOnly, 2);
    bind(&mut direct_config);
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

    let pilot = new_pilot();
    let resumed_root = temp.join("resumed");
    let mut resumed_config = config(&resumed_root, "b1", Segment::BuilderOnly, 2);
    bind(&mut resumed_config);
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

    resumed_config.resume = Some(manifest_resume(&temp.join("manifest.md"), "b1", 1));
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
        tripwire_games_for_test(&resumed_root).expect("games"),
        2_240
    );
    let stop: serde_json::Value = serde_json::from_slice(
        &fs::read(resumed_root.join("stops/stop-0000.json")).expect("window stop"),
    )
    .expect("window stop JSON");
    assert_eq!(stop["firing"]["denominator"], 2_240);
    assert_eq!(stop["first_watchdog_pod"], PODS_PER_UPDATE);
    assert_eq!(stop["first_watchdog_game"], 0);
    let receipt = read_value(&resumed_root.join("updates/update-0000/update.json"));
    assert!(
        receipt["optimizer"]["gradient_norms"]["build"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(
        receipt["optimizer"]["statistics"]["build_advantage"]["count"],
        192
    );
    assert_eq!(
        receipt["optimizer"]["statistics"]["entropy"]["count"]["count"],
        192
    );
    assert!(
        receipt["game_statistics"]["halts_by_stop_kind_and_attempt"]["driver_decision_cap"]["0"]
            .as_u64()
            .unwrap()
            >= 5
    );
    let bands = receipt["game_statistics"]["cap_hits_by_stop_kind_and_basics_band"]
        ["driver_decision_cap"]
        .as_object()
        .unwrap();
    let cap_hits_by_deck: u64 = bands.values().map(|value| value.as_u64().unwrap()).sum();
    assert_eq!(
        cap_hits_by_deck,
        2 * receipt["game_statistics"]["halts_by_stop_kind_and_attempt"]["driver_decision_cap"]
            ["0"]
            .as_u64()
            .unwrap()
    );
    assert!(receipt["game_statistics"]["replays"].as_u64().unwrap() >= 5);
    assert_eq!(receipt["checkpoint_number"], 1);
    assert_eq!(
        read_value(&resumed_root.join("updates/update-0000/commit.json"))["batch_index"],
        0
    );
    assert!(stop["firing"]["games"].as_u64().unwrap() >= 5);
    drop(direct);
    drop(resumed);
    assert_eq!(
        directory_bytes(&uninterrupted_root),
        directory_bytes(&resumed_root)
    );

    let update_dir = resumed_root.join("updates/update-0001");
    let mut exact_stop = fs::read(update_dir.join("stop.json")).unwrap();
    let ledger = resumed_root.join("stops/stop-0000.json");
    assert_eq!(fs::read(&ledger).unwrap(), exact_stop);
    // Valid JSON with different whitespace must be copied byte for byte.
    exact_stop.extend_from_slice(b"  \n");
    rewrite_committed_file(&update_dir, "stop.json", &exact_stop);
    fs::write(&ledger, b"{").expect("simulate a torn ledger copy");
    let error = TrainingDriver::open(
        config(&resumed_root, "b1", Segment::BuilderOnly, 2),
        model_rows(),
        1234,
    )
    .err()
    .expect("recovery restores the stop then refuses");
    assert_eq!(error, TrainingDriverError::StoppedWithoutResolution);
    assert_eq!(fs::read(ledger).unwrap(), exact_stop);
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
    let update = "updates/update-0000";
    for name in ["pods.json", "checkpoint.json"] {
        assert_eq!(
            fs::read(rerun_root.join(update).join(name)).unwrap(),
            fs::read(direct_root.join(update).join(name)).unwrap()
        );
    }
    let void = fs::read(rerun_root.join("watchdog/update-0000.json")).unwrap();
    let copied = fs::read(rerun_root.join(update).join("execution-0.json")).unwrap();
    assert_eq!(copied, void);
    let record: serde_json::Value = serde_json::from_slice(&void).unwrap();
    assert_eq!(record["kind"], "watchdog_void");
    assert_eq!(record["attempts"][0]["stop_kind"], "terminal:watchdog");
    let receipt = read_value(&rerun_root.join(update).join("update.json"));
    assert_eq!(receipt["execution"], 1);
    assert_eq!(receipt["void_execution_sha256"], sha256_hex(&void));
    let commit = read_value(&rerun_root.join(update).join("commit.json"));
    assert_eq!(commit["files"]["execution-0.json"], sha256_hex(&void));
    assert!(!rerun_root.join(update).join("partial").exists());

    let interrupted = temp.join("interrupted");
    fs::create_dir_all(interrupted.join("watchdog")).unwrap();
    fs::create_dir_all(interrupted.join("pending/update-0000")).unwrap();
    fs::write(interrupted.join("watchdog/update-0000.json"), &void).unwrap();
    let before = directory_bytes(&interrupted);
    let config = config(&interrupted, "b1", Segment::BuilderOnly, 1);
    for _ in 0..2 {
        let error = TrainingDriver::open(config.clone(), model_rows(), 1234)
            .err()
            .unwrap();
        assert_eq!(error, TrainingDriverError::ResumeAuthorizationRequired);
    }
    assert_eq!(directory_bytes(&interrupted), before);
    assert_eq!(
        fs::read(interrupted.join("watchdog/update-0000.json")).unwrap(),
        void
    );
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

fn manifest_resume(manifest: &Path, run: &str, next: u64) -> ResumeAuthorization {
    let entry = serde_json::json!({
        "kind": "resume",
        "run": run,
        "host_loss_evidence": "fixture host boot record",
        "last_published_update": next.checked_sub(1),
        "next_untouched_batch": next,
    });
    let bytes = format!("# Shakedown manifest\n\n```json\n{entry}\n```\n");
    fs::write(manifest, &bytes).unwrap();
    ResumeAuthorization {
        manifest: manifest.to_owned(),
        manifest_sha256: sha256_hex(bytes.as_bytes()),
        entry_line: 4,
    }
}

fn exit_after_one_update(root: &Path) {
    let name = "host_loss_resume_with_pinned_manifest_is_bit_identical";
    let test = module_path!().split_once("::").map_or_else(
        || name.to_owned(),
        |(_, module)| format!("{module}::{name}"),
    );
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test])
        .env("D6_TEST_UNRECORDED_EXIT_ROOT", root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(86), "{output:?}");
}

#[test]
fn unrecorded_exit_requires_manifest_without_writes() {
    let temp = TempTestDir::new("resume-refusal");
    let root = temp.join("b1");
    exit_after_one_update(&root);
    assert!(root.join("runner-active.json").exists());
    assert!(root.join("updates/update-0000/commit.json").exists());
    assert!(!root.join("pending/update-0001").exists());
    let before = directory_bytes(&root);
    let config = config(&root, "b1", Segment::BuilderOnly, 2);
    let refuse = |config: RunConfig| {
        let error = TrainingDriver::open(config, model_rows(), 1234)
            .err()
            .unwrap();
        assert_eq!(directory_bytes(&root), before);
        error
    };
    assert_eq!(
        refuse(config.clone()),
        TrainingDriverError::ResumeAuthorizationRequired
    );

    let mut unauthorized = config.clone();
    let mut bad_hash = manifest_resume(&temp.join("wrong-hash.md"), "b1", 1);
    bad_hash.manifest_sha256 = "11".repeat(32);
    unauthorized.resume = Some(bad_hash);
    assert!(refuse(unauthorized.clone()).to_string().contains("sha256"));
    for (run, batch) in [("wrong-run", 1), ("b1", 0)] {
        unauthorized.resume = Some(manifest_resume(&temp.join("wrong-entry.md"), run, batch));
        assert!(refuse(unauthorized.clone())
            .to_string()
            .contains("run or next untouched batch"));
    }
}

#[test]
fn host_loss_resume_with_pinned_manifest_is_bit_identical() {
    let pilot = FixturePilot(Arc::new(NaturalPilot));
    let cube = pool();
    if let Some(root) = std::env::var_os("D6_TEST_UNRECORDED_EXIT_ROOT") {
        let config = config(Path::new(&root), "b1", Segment::BuilderOnly, 2);
        let mut driver = TrainingDriver::open(config, model_rows(), 1234).unwrap();
        assert_eq!(
            driver.run_next_update(&cube, &pilot),
            Ok(RunOutcome::Published { update: 0 })
        );
        // Exit without running the driver's destructors, as on host loss.
        std::process::exit(86);
    }

    let temp = TempTestDir::new("pinned-manifest");
    let direct_root = temp.join("direct");
    let direct = config(&direct_root, "b1", Segment::BuilderOnly, 2);
    let mut direct = TrainingDriver::open(direct, model_rows(), 1234).unwrap();
    for update in 0..2 {
        assert_eq!(
            direct.run_next_update(&cube, &pilot),
            Ok(RunOutcome::Published { update })
        );
    }
    drop(direct);

    let resumed_root = temp.join("resumed");
    exit_after_one_update(&resumed_root);
    let mut resumed = config(&resumed_root, "b1", Segment::BuilderOnly, 2);
    resumed.resume = Some(manifest_resume(
        &temp.join("SHAKEDOWN-MANIFEST.md"),
        "b1",
        1,
    ));
    let mut resumed = TrainingDriver::open(resumed, model_rows(), 1234).unwrap();
    assert_eq!(resumed.next_update(), 1);
    assert_eq!(
        resumed.run_next_update(&cube, &pilot),
        Ok(RunOutcome::Published { update: 1 })
    );
    drop(resumed);
    assert_eq!(
        directory_bytes(&resumed_root),
        directory_bytes(&direct_root)
    );
}

#[test]
fn resume_never_replays_a_touched_batch() {
    let temp = TempTestDir::new("abandoned");
    let root = temp.join("b1");
    fs::create_dir_all(root.join("pending/update-0000")).unwrap();
    fs::write(root.join("pending/update-0000/commit.json"), b"{").unwrap();
    let before = directory_bytes(&root);
    let mut config = config(&root, "b1", Segment::BuilderOnly, 1);
    let error = TrainingDriver::open(config.clone(), model_rows(), 1234)
        .err()
        .unwrap();
    assert_eq!(error, TrainingDriverError::ResumeAuthorizationRequired);
    assert_eq!(directory_bytes(&root), before);

    config.resume = Some(manifest_resume(&temp.join("manifest.md"), "b1", 0));
    let error = TrainingDriver::open(config, model_rows(), 1234)
        .err()
        .unwrap();
    assert!(error.to_string().contains("batch that was touched"));
    assert_eq!(directory_bytes(&root), before);
}

#[test]
fn read_only_loader_checks_checkpoint_and_card_pins() {
    let temp = TempTestDir::new("read-only");
    let root = temp.join("b1");
    let mut ids = basic_land_ids().unwrap().to_vec();
    ids.sort_unstable();
    let database = format!("{KERNEL_CARDDB_HASH:016x}");
    let cards: Vec<_> = ids
        .iter()
        .map(|&id| {
            serde_json::json!({
                "card_id": id, "name": CARD_DEFS[usize::from(id)].name, "vector": [1]
            })
        })
        .collect();
    let table_data = serde_json::to_vec(&serde_json::json!({
        "schema": "card_semantics_table/v1", "kernel_carddb_hash": database,
        "vector_width": 1, "card_count": cards.len(),
        "vocabulary": [{"index": 0, "token": "land"}], "cards": cards
    }))
    .unwrap();
    let table_pin = sha256_hex(&table_data);
    let table = CubeTableSourceV1::from_bytes_v1(&table_data, &table_pin)
        .unwrap()
        .bind_v1(&CardIndexV1::from_ids(ids.clone()).unwrap())
        .unwrap();
    let rows = Arc::new(
        ModelCardRowsV1::with_registry_basics_v1(
            ids.clone(),
            table.vector_width(),
            table.scaled_rows().to_vec(),
            table.dense_columns().len(),
            table.dense_rows().to_vec(),
        )
        .unwrap(),
    );
    let initial =
        ModelV1::initialise_v1(rows.rows(), rows.table_width(), rows.dense_width(), 1234).unwrap();
    let mut loaded_config = config(&root, "b1", Segment::BuilderOnly, 1);
    loaded_config.pins.card_index = Some(table.index_pin().to_owned());
    loaded_config.pins.cube_table = Some(table_pin);
    loaded_config.metadata.card_database_sha256 = database;
    let driver = TrainingDriver::open(loaded_config, rows.clone(), 1234).unwrap();
    let path = root.join("initial/checkpoint.json");
    let before = directory_bytes(&root);
    let (checkpoint, model, loaded_rows) = load_checkpoint_read_only_v1(&path, &table).unwrap();
    assert_eq!(
        checkpoint.state_sha256,
        driver.checkpoint().unwrap().state_sha256
    );
    assert_eq!(model.parameter_sha256(), initial.parameter_sha256());
    assert_eq!(loaded_rows.ids(), &ids);
    for row in 0..rows.rows() {
        assert_eq!(loaded_rows.scaled_row(row), rows.scaled_row(row));
        assert_eq!(loaded_rows.dense_row(row), rows.dense_row(row));
    }
    assert_eq!(directory_bytes(&root), before);

    for field in ["card index pin", "cube table pin", "card database pin"] {
        let mut mismatch = config(&temp.join(field), "b1", Segment::BuilderOnly, 1);
        mismatch.pins.card_index = Some(table.index_pin().to_owned());
        mismatch.pins.cube_table = Some(table.file_sha256().to_owned());
        mismatch.metadata.card_database_sha256 = format!("{KERNEL_CARDDB_HASH:016x}");
        match field {
            "card index pin" => mismatch.pins.card_index = Some("00".repeat(32)),
            "cube table pin" => mismatch.pins.cube_table = Some("00".repeat(32)),
            _ => mismatch.metadata.card_database_sha256 = "0".repeat(16),
        }
        let checkpoint_path = mismatch.root.join("initial/checkpoint.json");
        TrainingDriver::open(mismatch, rows.clone(), 1234).unwrap();
        let error = load_checkpoint_read_only_v1(&checkpoint_path, &table).unwrap_err();
        assert!(error.to_string().contains(field), "{field}: {error}");
    }
    let original: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let zero = serde_json::json!("00".repeat(32));
    for (pointer, replacement, error) in [
        ("/schema", serde_json::json!("wrong"), "schema or state"),
        ("/state_sha256", zero.clone(), "schema or state"),
        (
            "/model/build/parameters/0",
            serde_json::json!(123),
            "schema or state",
        ),
        ("/parameter_layout_sha256", zero.clone(), "schema or state"),
        ("/card_index_sha256", zero.clone(), "schema or state"),
        ("/cube_table_sha256", zero, "schema or state"),
        (
            "/card_database_sha256",
            serde_json::json!("0000000000000000"),
            "schema or state",
        ),
        (
            "/card_index_sha256",
            serde_json::Value::Null,
            "schema or state",
        ),
    ] {
        let mut tampered = original.clone();
        *tampered.pointer_mut(pointer).unwrap() = replacement;
        let path = temp.join("tampered.json");
        let bytes = serde_json::to_vec(&tampered).unwrap();
        fs::write(&path, &bytes).unwrap();
        let failure = load_checkpoint_read_only_v1(&path, &table).unwrap_err();
        assert!(failure.to_string().contains(error), "{pointer}: {failure}");
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn same_game_watchdog_repeat_fires_in_q9() {
    let temp = TempTestDir::new("hang");
    let root = temp.join("b1");
    let pilot = FixturePilot(Arc::new(RepeatWatchdogPilot {
        calls: AtomicUsize::new(0),
        second_call: 1,
    }));
    let mut driver = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .unwrap();
    assert_eq!(
        driver.run_next_update(&pool(), &pilot),
        Ok(RunOutcome::Stopped {
            update: 0,
            kind: "watchdog_stop".into(),
        })
    );
    assert!(!root.join("updates/update-0000").exists());
    let stop: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("stops/stop-0000.json")).unwrap()).unwrap();
    assert_eq!(stop["hang"], true);
    assert_eq!(stop["firing"]["rule"]["target"], "hang");
    assert_eq!(stop["firing"]["games"], 1);
}

#[test]
fn only_a_matching_pinned_census_summary_enables_full_mode() {
    let temp = TempTestDir::new("baseline");
    let path = temp.join("summary.json");
    let bytes = br#"{"games":1,"completed":1,"halted":0,"stop_kinds":[],"cards":[],"order_errors":[],"stop_kind_mismatches":[]}"#;
    fs::write(&path, bytes).unwrap();
    let baseline = CensusBaselinePin {
        run: "census-b-r2".into(),
        summary_sha256: sha256_hex(bytes),
        manifest_sha256: "11".repeat(32),
        build_sha256: "22".repeat(32),
        fence_sha256: "33".repeat(32),
        authorization_sha256: "44".repeat(32),
    };
    let make_config = |root: &Path| {
        let mut config = config(root, "b1", Segment::BuilderOnly, 1);
        config.pins.binary = Some(baseline.build_sha256.clone());
        config.pins.fence_record = Some(baseline.fence_sha256.clone());
        config.tripwire = TripwireMode::Full {
            baseline: baseline.clone(),
        };
        config.tripwire_summary = Some(path.clone());
        config
    };

    for (name, binary_matches) in [("wrong-build", false), ("wrong-fence", true)] {
        let root = temp.join(name);
        let mut config = make_config(&root);
        if binary_matches {
            config.pins.fence_record = None;
        } else {
            config.pins.binary = None;
        }
        TrainingDriver::open(config, model_rows(), 1234).unwrap();
        let run: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("run.json")).unwrap()).unwrap();
        assert_eq!(run["tripwire"], serde_json::json!({"mode":"q9"}));
    }
    let root = temp.join("full");
    TrainingDriver::open(make_config(&root), model_rows(), 1234).unwrap();
    let run: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("run.json")).unwrap()).unwrap();
    assert_eq!(
        run["tripwire"]["baseline"]["summary_sha256"],
        baseline.summary_sha256
    );
    let q9_root = temp.join("q9-stays-q9");
    let mut q9 = make_config(&q9_root);
    q9.tripwire = TripwireMode::Q9;
    q9.tripwire_summary = None;
    TrainingDriver::open(q9, model_rows(), 1234).unwrap();
    let error = TrainingDriver::open(make_config(&q9_root), model_rows(), 1234)
        .err()
        .unwrap();
    assert!(error
        .to_string()
        .contains("resume changed a run pin, plan or tripwire mode"));

    fs::write(&path, b"changed bytes").unwrap();
    let error = TrainingDriver::open(make_config(&root), model_rows(), 1234)
        .err()
        .unwrap();
    assert!(error.to_string().contains("summary digest"));
}

fn read_value(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn pick_phase_requires_a_boundary_and_recovers_interrupted_start() {
    let temp = TempTestDir::new("boundary-recovery");
    let root = temp.join("k1");
    let picks = config(&root, "k1", Segment::PickPhase, 1);
    let before = TrainingDriver::open(picks.clone(), model_rows(), 1234)
        .err()
        .unwrap();
    assert!(before
        .to_string()
        .contains("requires a staged builder boundary"));
    assert!(!root.join("run.json").exists());

    let mut builder = TrainingDriver::open(
        config(&temp.join("b1"), "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .unwrap();
    let pilot = FixturePilot(Arc::new(NaturalPilot));
    assert_eq!(
        builder.run_next_update(&pool(), &pilot),
        Ok(RunOutcome::Published { update: 0 })
    );
    let expected_builder = builder.checkpoint().unwrap();
    let ready = TrainingDriver::start_pick_phase(&builder, picks.clone()).unwrap();
    let expected = directory_bytes(&root);
    let first = ready.checkpoint().unwrap();
    let source = first.boundary.unwrap();
    assert_eq!(source.builder_run, "b1");
    assert_eq!(
        source.builder_final_state_sha256,
        expected_builder.state_sha256
    );
    drop(ready);

    // The journal was published, but neither destination had been installed.
    fs::remove_file(root.join("initial/checkpoint.json")).unwrap();
    fs::remove_file(root.join("run.json")).unwrap();
    let restored = TrainingDriver::open(picks, model_rows(), 9999).unwrap();
    assert_eq!(
        restored.checkpoint().unwrap().state_sha256,
        first.state_sha256
    );
    assert_eq!(directory_bytes(&root), expected);
}

#[test]
fn checkpoint_hash_covers_heads_lineage_league_and_pin_reasons() {
    let temp = TempTestDir::new("header-tamper");
    let root = temp.join("b1");
    let config = config(&root, "b1", Segment::BuilderOnly, 1);
    TrainingDriver::open_with_lineage(
        config.clone(),
        model_rows(),
        1234,
        vec![LineageSegment {
            segment: Segment::BuilderOnly,
            run: "parent".into(),
            pilots: vec![mtg_kernel::cube_pod_runner_v1::PilotKind::Checkpoint],
        }],
    )
    .unwrap();
    let path = root.join("initial/checkpoint.json");
    let original = read_value(&path);
    for (field, replacement) in [
        ("/head_updates/pick", serde_json::json!(42)),
        ("/lineage", serde_json::json!([])),
        ("/league", serde_json::json!([])),
        ("/pin_reasons", serde_json::json!([])),
    ] {
        let mut modified = original.clone();
        *modified.pointer_mut(field).unwrap() = replacement;
        fs::write(&path, serde_json::to_vec(&modified).unwrap()).unwrap();
        let error = TrainingDriver::open(config.clone(), model_rows(), 1234)
            .err()
            .unwrap();
        assert!(
            error.to_string().contains("schema or state digest"),
            "{field}: {error}"
        );
        let error = prune_checkpoint(&path).unwrap_err();
        assert!(
            error.to_string().contains("schema or state digest"),
            "{field}: {error}"
        );
    }
}

#[test]
fn stack_cap_and_liveness_are_in_the_run_record_and_stack_cap_reaches_pods() {
    use mtg_kernel::cube_b_census_v1::A2_CAPS;
    let temp = TempTestDir::new("stack-cap");
    let root = temp.join("b1");
    let mut driver = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .unwrap();
    let run = read_value(&root.join("run.json"));
    assert_eq!(
        run["caps"]["max_stack_objects"],
        serde_json::json!(A2_CAPS.max_stack_objects)
    );
    assert_eq!(
        run["liveness"],
        mtg_kernel::cube_pod_runner_v1::WATCHDOG_LIVENESS
    );
    let expected = A2_CAPS.max_stack_objects;
    struct CapPilot(Option<u32>);
    impl GamePilot for CapPilot {
        fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
            assert_eq!(caps.max_stack_objects, self.0);
            AlwaysHaltPilot.play(spec, caps)
        }
    }
    let result = driver
        .run_next_update(&pool(), &FixturePilot(Arc::new(CapPilot(expected))))
        .unwrap();
    assert_eq!(
        result,
        RunOutcome::Stopped {
            update: 0,
            kind: "retry_exhausted".into()
        }
    );
    let stop = read_value(&root.join("stops/stop-0000.json"));
    assert_eq!(
        stop["batch"][0]["caps"]["max_stack_objects"],
        serde_json::json!(expected)
    );
}

#[test]
fn checkpoint_sixteen_is_pinned_at_creation_and_recovered_after_copy_loss() {
    let temp = TempTestDir::new("league-sixteen");
    let root = temp.join("b1");
    let config = config(&root, "b1", Segment::BuilderOnly, 17);
    let mut driver = TrainingDriver::open(config.clone(), model_rows(), 1234).unwrap();
    let pilot = FixturePilot(Arc::new(NaturalPilot));
    let cube = pool();
    let initial_snapshot = root.join(&driver.league()[0].path);
    let moved = initial_snapshot.with_extension("test-cache");
    for update in 0..16 {
        // After the first validated load, a second update must use the cached model.
        if update == 1 {
            fs::rename(&initial_snapshot, &moved).unwrap();
        }
        let result = driver.run_next_update(&cube, &pilot);
        if update == 1 {
            fs::rename(&moved, &initial_snapshot).unwrap();
        }
        assert_eq!(result, Ok(RunOutcome::Published { update }));
    }
    assert_eq!(
        driver
            .league()
            .iter()
            .map(|entry| entry.update)
            .collect::<Vec<_>>(),
        [0, 16]
    );
    let snapshot_pin = driver.league()[1].pin.clone();
    let snapshot = root.join(&driver.league()[1].path);
    let snapshot_bytes = fs::read(&snapshot).unwrap();
    let staged = root.join("updates/update-0015/league.json");
    assert_eq!(fs::read(staged).unwrap(), snapshot_bytes);
    assert_eq!(
        read_value(&snapshot)["pin_reasons"],
        serde_json::json!(["league"])
    );
    assert_eq!(
        prune_checkpoint(&snapshot),
        Err(TrainingDriverError::PinnedCheckpoint)
    );
    let commit = read_value(&root.join("updates/update-0015/commit.json"));
    assert_eq!(commit["files"]["league.json"], sha256_hex(&snapshot_bytes));

    // Restore the content-addressed copy and resume after pruning a historical checkpoint.
    drop(driver);
    fs::remove_file(&snapshot).unwrap();
    let old = root.join("updates/update-0000/checkpoint.json");
    prune_checkpoint(&old).unwrap();
    assert!(!old.exists());
    assert!(old.with_file_name("checkpoint.pruned").exists());

    let mut reopened = TrainingDriver::open(config, model_rows(), 1234).unwrap();
    assert_eq!(fs::read(&snapshot).unwrap(), snapshot_bytes);
    assert_eq!(reopened.next_update(), 16);
    assert_eq!(
        reopened.run_next_update(&cube, &pilot),
        Ok(RunOutcome::Published { update: 16 })
    );
    let update = root.join("updates/update-0016");
    let receipt = read_value(&update.join("update.json"));
    assert!(receipt["league_entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|pin| pin == &snapshot_pin));
    let pods: Vec<serde_json::Value> = serde_json::from_reader(flate2::read::GzDecoder::new(
        fs::File::open(update.join("pods.json")).unwrap(),
    ))
    .unwrap();
    assert!(pods
        .iter()
        .flat_map(|pod| pod["decks"].as_array().unwrap())
        .any(|deck| deck["builder"]["pin"] == snapshot_pin));
}

#[test]
fn optimizer_failure_keeps_its_true_stop_kind() {
    use mtg_kernel::cube_training_driver_v1::Checkpoint;
    let temp = TempTestDir::new("failed-update");
    let root = temp.join("b1");
    let config = config(&root, "b1", Segment::BuilderOnly, 1);
    TrainingDriver::open(config.clone(), model_rows(), 1234).unwrap();
    let path = root.join("initial/checkpoint.json");
    let mut checkpoint: Checkpoint = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    checkpoint.model.build.adam_step = u64::MAX;
    checkpoint.state_sha256.clear();
    let mut canonical = b"mtg-kernel-cube-checkpoint-state/v1\0".to_vec();
    serde_json::to_writer(&mut canonical, &checkpoint).unwrap();
    checkpoint.state_sha256 = sha256_hex(&canonical);
    fs::write(&path, serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    let mut driver = TrainingDriver::open(config.clone(), model_rows(), 1234).unwrap();
    assert!(matches!(
        driver.run_next_update(&pool(), &FixturePilot(Arc::new(NaturalPilot))),
        Err(TrainingDriverError::Update(_))
    ));
    let stop_path = root.join("stops/stop-0000.json");
    let stop = read_value(&stop_path);
    assert_eq!(stop["kind"], "update_failed");
    assert!(stop["error"]
        .as_str()
        .unwrap()
        .contains("Adam counter overflow"));
    assert!(!root.join("pending/update-0000").exists());
    assert_eq!(
        TrainingDriver::open(config, model_rows(), 1234)
            .err()
            .unwrap(),
        TrainingDriverError::StoppedWithoutResolution
    );
    assert_eq!(read_value(&stop_path), stop);
}

#[test]
fn shakedown_label_and_switch_are_validated() {
    let temp = TempTestDir::new("plan-label");
    let mut invalid_label = config(&temp.join("label"), "b1", Segment::BuilderOnly, 1);
    invalid_label.label = "shakedown".into();
    let error = TrainingDriver::open(invalid_label, model_rows(), 1234)
        .err()
        .unwrap();
    assert!(error.to_string().contains("shakedown label and namespace"));
    let mut invalid_switch = config(&temp.join("switch"), "b1", Segment::BuilderOnly, 2);
    invalid_switch.pilot.switch = Some(PilotSwitch {
        update: 1,
        gate: Some(pilot_pin()),
    });
    let error = TrainingDriver::open(invalid_switch, model_rows(), 1234)
        .err()
        .unwrap();
    assert!(error.to_string().contains("nonboundary pilot switch"));
}

#[test]
fn retry_exhaustion_after_a_watchdog_keeps_the_first_expiry() {
    struct WatchdogThenCap(AtomicBool);
    impl GamePilot for WatchdogThenCap {
        fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
            let mut outcome = AlwaysHaltPilot.play(spec, caps);
            if !self.0.swap(true, Ordering::SeqCst) {
                outcome.end = watchdog_game_end();
            }
            outcome
        }
    }
    let temp = TempTestDir::new("watchdog-then-cap");
    let root = temp.join("b1");
    let mut driver = TrainingDriver::open(
        config(&root, "b1", Segment::BuilderOnly, 1),
        model_rows(),
        1234,
    )
    .unwrap();
    let pilot = FixturePilot(Arc::new(WatchdogThenCap(AtomicBool::new(false))));
    assert_eq!(
        driver.run_next_update(&pool(), &pilot),
        Ok(RunOutcome::Stopped {
            update: 0,
            kind: "retry_exhausted".into(),
        })
    );
    let stop = read_value(&root.join("stops/stop-0000.json"));
    assert_eq!(stop["first_watchdog_pod"], 0);
    assert_eq!(stop["first_watchdog_game"], 0);
    let original = read_value(&root.join("watchdog/update-0000.json"));
    assert_eq!(original["attempts"][0]["stop_kind"], "terminal:watchdog");
    assert!(!root.join("updates/update-0000").exists());
}

fn rewrite_committed_file(update: &Path, name: &str, bytes: &[u8]) {
    let commit_path = update.join("commit.json");
    let mut commit = read_value(&commit_path);
    commit["files"][name] = serde_json::json!(sha256_hex(bytes));
    fs::write(update.join(name), bytes).unwrap();
    fs::write(commit_path, serde_json::to_vec(&commit).unwrap()).unwrap();
}

#[test]
fn resume_checks_receipt_binding_and_rejects_missing_game_fields() {
    use std::io::Write;
    let temp = TempTestDir::new("invalid-published");
    let root = temp.join("b1");
    let config = config(&root, "b1", Segment::BuilderOnly, 2);
    let mut driver = TrainingDriver::open(config.clone(), model_rows(), 1234).unwrap();
    let pilot = FixturePilot(Arc::new(NaturalPilot));
    assert_eq!(
        driver.run_next_update(&pool(), &pilot),
        Ok(RunOutcome::Published { update: 0 })
    );
    let update = root.join("updates/update-0000");
    let original_receipt = fs::read(update.join("update.json")).unwrap();
    let mut receipt = read_value(&update.join("update.json"));
    receipt["after_state_sha256"] = serde_json::json!("00".repeat(32));
    rewrite_committed_file(
        &update,
        "update.json",
        &serde_json::to_vec(&receipt).unwrap(),
    );
    let error = TrainingDriver::open(config.clone(), model_rows(), 1234)
        .err()
        .unwrap();
    assert!(error
        .to_string()
        .contains("differs from its published update receipt"));
    rewrite_committed_file(&update, "update.json", &original_receipt);

    let mut pods: Vec<serde_json::Value> = serde_json::from_reader(flate2::read::GzDecoder::new(
        fs::File::open(update.join("pods.json")).unwrap(),
    ))
    .unwrap();
    pods[0].as_object_mut().unwrap().remove("games");
    let mut bytes = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    bytes
        .write_all(&serde_json::to_vec(&pods).unwrap())
        .unwrap();
    rewrite_committed_file(&update, "pods.json", &bytes.finish().unwrap());
    let error = TrainingDriver::open(config, model_rows(), 1234)
        .err()
        .unwrap();
    assert!(error.to_string().contains("missing field") && error.to_string().contains("games"));
}

#[test]
fn signed_shakedown_checkpoint_marks_match_a6_and_d0() {
    let b1 = RunConfig::shakedown(PathBuf::new(), "b1", Segment::BuilderOnly);
    let k1 = RunConfig::shakedown(PathBuf::new(), "k1", Segment::PickPhase);
    assert_eq!(b1.marks.dose_reads, [32, 64, 96, 128]);
    assert_eq!(b1.marks.owner_views, b1.marks.dose_reads);
    assert!(k1.marks.dose_reads.is_empty());
    assert_eq!(k1.marks.owner_views, [64, 128]);
}
