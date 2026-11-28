use super::*;
use mtg_kernel::cube_draft_build_v1::model::ModelCardRowsV1;
use mtg_kernel::cube_draft_v1::{game_seed, pod_game_seats, pod_pairings, SEATS};
use mtg_kernel::cube_pod_runner_v1::{
    pod_draft_seed, pod_play_seed, BuilderRef, ChairRole, DeckSource, GameRole, PodSeeds, SeatKind,
    SeatRef, SeatRule,
};
use mtg_kernel::cube_read_plan_v1::{
    compute_plan_set_sha256, run_plan_set_execution, supervise_plan_set_execution, DeckRow,
    ExecutionReason, ManifestEntry, ManifestExecutionEntry, ManifestKind, ManifestPodEntry,
    ManifestUnitEntry, PlanChair, PlanDeck, PlanFamily, PlanGame, PlanInputs, PrepBuildSpec,
    PrepFamily, PrepRequest, ReadPlanV1, SetManifest, READ_PLAN_SCHEMA,
};
use mtg_kernel::cube_seat_v1::{UniformPilot, M4_INTERFACES_VERSION};
use mtg_kernel::cube_training_driver_v1::{RunConfig, Segment, TrainingDriver};
use std::{fs, sync::Arc};

pub(super) struct TempDir(pub(super) PathBuf);
impl TempDir {
    pub(super) fn new(label: &str) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("d7-{label}-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Prep,
    Inline,
    Supervised,
    Worker,
}

fn plan(unit: u64, pins: &PodPins, picker: SeatRef) -> ReadPlanV1 {
    let pod = pod_play_seed("test", "v1", unit).unwrap();
    ReadPlanV1 {
        schema: READ_PLAN_SCHEMA.into(),
        interfaces: M4_INTERFACES_VERSION.into(),
        family: PlanFamily::AAll,
        pod_use: PodUse::Test,
        namespace: "test".into(),
        run: "v1".into(),
        unit,
        pod_index: unit,
        mirror: None,
        mirror_pods: None,
        rotation: None,
        mark: None,
        seeds: PodSeeds::for_read(
            pod_draft_seed("test", "v1", unit).unwrap(),
            pod,
            None,
            "test",
            "test",
        ),
        pins: pins.clone(),
        pilot: PilotInfo::uniform(),
        caps: PodCaps::read_a2(),
        seat_rule: SeatRule::Schedule,
        inputs: PlanInputs::default(),
        prep_sha256: None,
        pools: None,
        paired_builds: vec![],
        chairs: (0..SEATS as u8)
            .map(|chair| PlanChair {
                chair,
                role: ChairRole::Learning,
                picker: picker.clone(),
            })
            .collect(),
        decks: (0..SEATS as u8)
            .map(|chair| PlanDeck {
                deck: chair.into(),
                source: DeckSource::Build,
                chair: Some(chair),
                build: Some(0),
                builder: BuilderRef::uniform(),
                cards: None,
                dealt: None,
                row: DeckRow::Candidate,
            })
            .collect(),
        games: (0..pod_pairings().len())
            .map(|g| {
                let seats = pod_game_seats(pod, g).unwrap();
                PlanGame {
                    game: g as u32,
                    group: g as u32,
                    role: GameRole::Cross,
                    pairing: Some(g as u8),
                    decks: seats.map(|s| s as u32),
                    chairs: seats.map(|s| Some(s as u8)),
                    seat_rule: SeatRule::Schedule,
                    game_seed: game_seed(pod, g),
                    episode: g as u64,
                }
            })
            .collect(),
    }
}

fn write_checkpoints(
    root: &Path,
    table: &mtg_kernel::cube_training_driver_v1::CubeTableV1,
) -> [(String, PathBuf); 2] {
    let rows = Arc::new(
        ModelCardRowsV1::with_registry_basics_v1(
            table.index().ids().to_vec(),
            table.vector_width(),
            table.scaled_rows().to_vec(),
            table.dense_columns().len(),
            table.dense_rows().to_vec(),
        )
        .unwrap(),
    );
    let checkpoint = |label: &str, seed| {
        let dir = root.join(label);
        let mut cfg = RunConfig::shakedown(&dir, "b1", Segment::BuilderOnly);
        cfg.pins.card_index = Some(table.index_pin().into());
        cfg.pins.cube_table = Some(table.file_sha256().into());
        cfg.metadata.card_database_sha256 =
            format!("{:016x}", mtg_kernel::card_def::KERNEL_CARDDB_HASH);
        let driver = TrainingDriver::open(cfg, rows.clone(), seed).unwrap();
        (
            driver.checkpoint_state_sha256().to_owned(),
            dir.join("initial/checkpoint.json"),
        )
    };
    [checkpoint("wanted", 31), checkpoint("other", 47)]
}

// Unit 0 is runnable without a checkpoint. Only unit 1 requires the missing
// state, so checking one unit at a time would publish a partial execution.
fn write_set(set: &Path, prep: bool, pins: &PodPins, wanted: &str) {
    let mut manifest = SetManifest {
        schema: READ_PLAN_SCHEMA.into(),
        kind: if prep {
            ManifestKind::Prep
        } else {
            ManifestKind::Plans
        },
        family: if prep { "b" } else { "a_all" }.into(),
        namespace: "test".into(),
        run: "v1".into(),
        set: "set".into(),
        interfaces: M4_INTERFACES_VERSION.into(),
        declaration_sha256: sha256_hex(b"fixture"),
        look: None,
        units: [0, 2],
        reserved: None,
        pools_with: None,
        panel_sha256: None,
        prep_set: None,
        replaces: None,
        retired: vec![],
        plans: vec![],
        plan_set_sha256: String::new(),
        executions: vec![ManifestExecutionEntry {
            execution: 0,
            reason: ExecutionReason::First,
            expired_games: vec![],
            binary: pins.binary.clone().unwrap(),
            platform: "local".into(),
            claim_id: None,
        }],
    };
    for unit in 0..2 {
        let picker = if unit == 1 && !prep {
            SeatRef {
                kind: SeatKind::Learned,
                pin: Some(wanted.to_owned()),
            }
        } else {
            SeatRef::uniform()
        };
        let p = plan(unit, pins, picker);
        let (file, bytes) = if prep {
            let builder = if unit == 1 {
                BuilderRef::learned(wanted)
            } else {
                BuilderRef::uniform()
            };
            let request = PrepRequest {
                schema: p.schema,
                kind: "prep_request".into(),
                family: PrepFamily::B,
                namespace: p.namespace,
                run: p.run,
                unit,
                seeds: p.seeds,
                pins: p.pins,
                chairs: p.chairs,
                builds: (0..SEATS as u8)
                    .map(|chair| PrepBuildSpec {
                        chair,
                        build: 0,
                        builder: builder.clone(),
                        row: DeckRow::Candidate,
                    })
                    .collect(),
            };
            (
                format!("requests/unit-{unit}.json"),
                to_canonical_pretty_json(&request).unwrap(),
            )
        } else {
            (
                format!("plans/pod-{unit}.json"),
                to_canonical_pretty_json(&p).unwrap(),
            )
        };
        fs::create_dir_all(set.join(&file).parent().unwrap()).unwrap();
        fs::write(set.join(&file), &bytes).unwrap();
        let sha256 = sha256_hex(bytes.as_bytes());
        manifest.plans.push(if prep {
            ManifestEntry::Unit(ManifestUnitEntry { unit, file, sha256 })
        } else {
            ManifestEntry::Pod(ManifestPodEntry {
                pod_index: unit,
                file,
                sha256,
            })
        });
    }
    manifest.plan_set_sha256 = compute_plan_set_sha256(&manifest.plans).unwrap();
    fs::write(
        set.join("manifest.json"),
        to_canonical_pretty_json(&manifest).unwrap(),
    )
    .unwrap();
}

fn refuses_before_publication(entry: Entry) {
    let temp = TempDir::new(&format!("preflight-{entry:?}"));
    let root = &temp.0;
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data");
    let cube = data.join("cube_v1/mtgo_vintage_cube_20261007.json");
    let gaps = data.join("cube_v1/mtgo_vintage_cube_20261007_gaps.json");
    let fence = root.join("fence.json");
    fs::write(&fence, "[]").unwrap();
    let table_path = data.join("card_semantics_v1/cube_table_v1.json");
    let source =
        CubeTableSourceV1::read_v1(&table_path, &sha256_hex(&fs::read(&table_path).unwrap()))
            .unwrap();
    let (index, _) =
        CardIndexV1::provisional_v1(&fs::read_to_string(&cube).unwrap(), &source).unwrap();
    let table = source.bind_v1(&index).unwrap();
    let [(wanted, wanted_path), (other, other_path)] = write_checkpoints(root, &table);
    assert_ne!(wanted, other);
    let pin = "a".repeat(64);
    let pins = PodPins {
        binary: Some(pin.clone()),
        cube_list: Some(sha256_hex(&fs::read(&cube).unwrap())),
        fence_record: Some(sha256_hex(b"[]")),
        card_index: Some(table.index_pin().into()),
        cube_table: Some(table.file_sha256().into()),
    };
    let set = root.join("set");
    write_set(&set, entry == Entry::Prep, &pins, &wanted);
    let decl = root.join("declaration");
    fs::write(&decl, b"fixture").unwrap();
    let output = root.join("worker-output");
    let mut args = Args {
        checkpoints: vec![],
        values: BTreeMap::new(),
    };
    for (flag, path) in [
        ("--cube", cube),
        ("--gaps", gaps),
        ("--fence", fence),
        ("--declaration", decl),
        ("--out", output.join("record")),
        ("--timing", output.join("timing")),
        ("--journal", output.join("journal")),
    ] {
        args.values.insert(flag.into(), path.display().to_string());
    }
    args.values.insert("--execution".into(), "0".into());
    args.values.insert("--binary-pin".into(), pin);
    args.values
        .insert("--cube-table-pin".into(), table.file_sha256().into());
    args.values
        .insert("--card-index-pin".into(), table.index_pin().into());
    let flag = match entry {
        Entry::Prep => "--prep-set",
        Entry::Worker => "--worker-plan",
        _ => "--plan-set",
    };
    let input = if entry == Entry::Worker {
        set.join("plans/pod-1.json")
    } else {
        set.clone()
    };
    args.values.insert(flag.into(), input.display().to_string());
    let mut loaded = load_pool(&args).unwrap();
    for wrong_checkpoint in [false, true] {
        if wrong_checkpoint {
            loaded.seats.load(&other_path, &other, &table, 1).unwrap();
        }
        let error = if entry == Entry::Worker {
            run_worker_plan(&args, &loaded).unwrap_err()
        } else {
            let verification = load_verification(&args, &loaded).unwrap();
            match entry {
                Entry::Prep => run_prep_set_execution_with_ended_claim(
                    &loaded.pool,
                    &set,
                    0,
                    &loaded.seats,
                    &verification,
                    None,
                    None,
                )
                .unwrap_err(),
                Entry::Inline => run_plan_set_execution(
                    &loaded.pool,
                    &set,
                    0,
                    &loaded.seats,
                    &UniformPilot,
                    &verification,
                    1,
                    None,
                )
                .unwrap_err(),
                Entry::Supervised => supervise_plan_set_execution(
                    &loaded.pool,
                    &set,
                    0,
                    &loaded.seats,
                    &verification,
                    &CommandPodWorkerLauncher::new(|_, _, _, _, _| {
                        panic!("worker launched before seat resolution")
                    }),
                )
                .unwrap_err(),
                Entry::Worker => unreachable!(),
            }
            .to_string()
        };
        assert!(
            error.contains("no loaded checkpoint") && error.contains(&wanted),
            "{error}"
        );
        assert!(!set.join("exec-0").exists());
        assert!(!set.join(".supervisor-exec-0").exists());
        assert!(!output.exists());
    }
    loaded.seats.load(&wanted_path, &wanted, &table, 1).unwrap();
    for pin in [wanted, other] {
        use mtg_kernel::cube_read_plan_v1::SeatFactory;
        loaded
            .seats
            .make_picker(&SeatRef {
                kind: SeatKind::Learned,
                pin: Some(pin.clone()),
            })
            .unwrap();
        loaded
            .seats
            .make_builder(&BuilderRef::learned(pin))
            .unwrap();
    }
}

#[test]
fn prep_refuses_unavailable_checkpoint_without_output() {
    refuses_before_publication(Entry::Prep);
}
#[test]
fn inline_plan_set_refuses_unavailable_checkpoint_without_output() {
    refuses_before_publication(Entry::Inline);
}
#[test]
fn supervised_plan_set_refuses_unavailable_checkpoint_without_output() {
    refuses_before_publication(Entry::Supervised);
}
#[test]
fn worker_refuses_unavailable_checkpoint_without_journal() {
    refuses_before_publication(Entry::Worker);
}
