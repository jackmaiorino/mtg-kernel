use super::*;
use mtg_kernel::cube_draft_v1::{basic_land_ids, GameCaps, GameEnd};
use mtg_kernel::cube_seat_v1::{AttemptOutcome, AttemptSpec};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static ID: AtomicUsize = AtomicUsize::new(0);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/d8-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn pinned(path: &Path) -> Value {
    json!({"path":path,"sha256":sha256_hex(&fs::read(path).unwrap())})
}
fn write_pin(root: &Path, name: &str, bytes: &[u8]) -> Value {
    let path = root.join(name);
    fs::write(&path, bytes).unwrap();
    pinned(&path)
}
fn append_entry(buffer: &mut String, seq: usize, kind: &str, mut body: Value) {
    body["seq"] = json!(seq);
    body["kind"] = json!(kind);
    body["time_et"] = json!("2026-11-25 12:00 ET");
    body["prefix_sha256"] = json!(sha256_hex(buffer.as_bytes()));
    if body.get("files").is_none() {
        body["files"] = json!({});
    }
    buffer.push_str(&format!("## Entry {seq}: {kind}\n\n```json\n{body}\n```\n"));
}
fn fixture(root: &Path) -> (Args, Value, Vec<u16>) {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data");
    let list = data.join("cube_v1/mtgo_vintage_cube_20261007.json");
    let cube = CubePool::from_json_texts(&fs::read_to_string(&list).unwrap(), "{}", None).unwrap();
    let mut ids = cube.ids().to_vec();
    ids.sort_unstable();
    assert_ne!(cube.ids(), ids);
    let mut manifest = String::new();
    for (i, k) in ["d0_signed", "a6_part2_countersigned", "panel", "fixtures"]
        .iter()
        .enumerate()
    {
        append_entry(&mut manifest, i + 1, k, json!({}));
    }
    let spec = json!({
        "cube_table":pinned(&data.join("card_semantics_v1/cube_table_v1.json")),
        "cube_list":pinned(&list),
        "ordered_pool":write_pin(root,"pool",format!("{}\n",json!(ids)).as_bytes()),
        "card_database":write_pin(root,"db",json!({"schema":"cube_card_ids/v1","kernel_carddb_hash":format!("{KERNEL_CARDDB_HASH:016x}")}).to_string().as_bytes()),
        "fence":write_pin(root,"fence",b"{\"cards\":[]}"),
        "read_plan":write_pin(root,"read-plan",b"fixture plan"),
        "manifest":write_pin(root,"manifest",manifest.as_bytes()),
        "manifest_prefix_sha256":sha256_hex(manifest.as_bytes()),
        "card_index_sha256":"fb2f6fa80f612fb100be983b1c2a4947f8c3add61aa2d81b187d8e49ea526b63",
        "binary_sha256":sha256_hex(&fs::read(env::current_exe().unwrap()).unwrap()),
        "initial_state_sha256":"90cff6d7f7c58ad606c71d14274457a2cbe2836e2113e192d8012eec9fd2016f"
    });
    let argv = [
        "prepare".into(),
        root.display().to_string(),
        "b1".into(),
        root.join("inputs").display().to_string(),
        "--threads".into(),
        "1".into(),
    ];
    (Args::parse(&argv).unwrap(), spec, ids)
}

fn prepare_and_authorize(args: &mut Args, spec: &mut Value) {
    fs::write(&args.inputs, spec.to_string()).unwrap();
    assert!(launch(args).unwrap());
    let pin = pinned(&args.root.join("b1/run.json"))["sha256"].clone();
    let mut manifest = fs::read_to_string(args.root.join("manifest")).unwrap();
    append_entry(
        &mut manifest,
        5,
        "training_run",
        json!({"run_id":"shakedown-pods/b1","tripwire":{"mode":"q9"},"files":{"b1/run.json":pin}}),
    );
    args.prepare = false;
    spec["manifest"] = write_pin(&args.root, "manifest", manifest.as_bytes());
    fs::write(&args.inputs, spec.to_string()).unwrap();
}

#[test]
fn each_pin_refuses_before_any_game() {
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    prepare_and_authorize(&mut args, &mut spec);
    for field in [
        "cube_table",
        "cube_list",
        "ordered_pool",
        "card_database",
        "fence",
        "read_plan",
        "manifest",
        "manifest_prefix_sha256",
        "card_index_sha256",
        "binary_sha256",
        "initial_state_sha256",
    ] {
        let mut wrong = spec.clone();
        if wrong[field].is_object() {
            wrong[field]["sha256"] = json!("0".repeat(64));
        } else {
            wrong[field] = json!("0".repeat(64));
        }
        fs::write(&args.inputs, wrong.to_string()).unwrap();
        let error = launch(&args).unwrap_err().to_string();
        assert!(
            error.contains("sha256") || error.contains("prefix") || error.contains("run pin"),
            "{field}: {error}"
        );
        assert!(!root.0.join("b1/updates/update-0000").exists());
        assert!(!root.0.join("b1/pending/update-0000").exists());
    }
}

#[test]
fn pool_uses_pinned_ascending_bytes() {
    let root = Scratch::new();
    let (_, mut spec, mut ids) = fixture(&root.0);
    let load = |s: Value| serde_json::from_value::<Inputs>(s).unwrap().load();
    assert_eq!(load(spec.clone()).unwrap().pool.ids(), ids);
    ids.swap(0, 1);
    spec["ordered_pool"] = write_pin(&root.0, "pool", format!("{}\n", json!(ids)).as_bytes());
    assert!(load(spec).err().unwrap().to_string().contains("ascending"));
}

struct Fixture;

#[test]
fn training_waits_for_its_own_manifest_entry() {
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    fs::write(&args.inputs, spec.to_string()).unwrap();
    assert!(launch(&args).unwrap());
    args.prepare = false;
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("missing training_run"));
    let prefix = fs::read_to_string(root.0.join("manifest")).unwrap();
    let actual = pinned(&root.0.join("b1/run.json"))["sha256"].clone();
    for (run, pin, mode, expected) in [
        (
            "shakedown-pods/k1",
            actual.clone(),
            "q9",
            "missing training_run",
        ),
        (
            "shakedown-pods/b1",
            json!("0".repeat(64)),
            "q9",
            "training_run differs",
        ),
        (
            "shakedown-pods/b1",
            actual.clone(),
            "full",
            "training_run differs",
        ),
    ] {
        let mut manifest = prefix.clone();
        append_entry(
            &mut manifest,
            5,
            "training_run",
            json!({"run_id":run,"tripwire":{"mode":mode},"files":{"run.json":pin}}),
        );
        spec["manifest"] = write_pin(&root.0, "manifest", manifest.as_bytes());
        fs::write(&args.inputs, spec.to_string()).unwrap();
        assert!(launch(&args).unwrap_err().to_string().contains(expected));
    }
    assert!(!root.0.join("b1/updates/update-0000").exists());
    assert!(!root.0.join("b1/runner-active.json").exists());
    let mut manifest = prefix;
    append_entry(
        &mut manifest,
        5,
        "training_run",
        json!({"run_id":"shakedown-pods/b1","tripwire":{"mode":"q9"},"files":{"b1/run.json":actual}}),
    );
    spec["manifest"] = write_pin(&root.0, "manifest", manifest.as_bytes());
    spec["manifest_prefix_sha256"] = spec["manifest"]["sha256"].clone();
    fs::write(&args.inputs, spec.to_string()).unwrap();
    args.prepare = true;
    args.run = "k1".into();
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("completed builder segment"));
    assert!(!root.0.join("k1/run.json").exists());
}

impl GamePilot for Fixture {
    fn play(&self, spec: &AttemptSpec<'_>, _: &GameCaps) -> AttemptOutcome {
        AttemptOutcome {
            decisions: 1,
            physical_decisions: Some(1),
            policy_steps: Some(1),
            end: GameEnd::Natural {
                winner_seat: Some(spec.seats[(spec.action_seed % 2) as usize]),
                reason: "fixture".into(),
            },
            last_action_source: None,
        }
    }
}
impl PilotProvider for Fixture {
    fn uniform_pilot(&self) -> Arc<dyn GamePilot + Sync> {
        Arc::new(Fixture)
    }
    fn load_checkpoint(
        &self,
        pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        UniformOnlyPilotProvider.load_checkpoint(pin)
    }
}

#[test]
fn two_updates_match_across_thread_counts() {
    let root = Scratch::new();
    let (_, spec, mut ids) = fixture(&root.0);
    ids.extend(basic_land_ids().unwrap());
    ids.sort_unstable();
    ids.dedup();
    let n = ids.len();
    let rows = Arc::new(
        ModelCardRowsV1::with_registry_basics_v1(ids, 1, vec![0.; n], 1, vec![0.; n]).unwrap(),
    );
    let run = |threads| {
        let run_root = root.0.join(format!("w{threads}"));
        fs::create_dir(&run_root).unwrap();
        let (mut args, _, _) = fixture(&run_root);
        args.threads = threads;
        let mut inputs: Inputs = serde_json::from_value(spec.clone()).unwrap();
        let mut data = inputs.load().unwrap();
        data.rows = rows.clone();
        let mut config = inputs.config(&args, &data).unwrap();
        config.update_range = [0, 2];
        let mut reference = config.clone();
        reference.root = run_root.join("init");
        let init = seed(&[
            SeedPart::Str("shakedown-pods"),
            SeedPart::Str("pod"),
            SeedPart::Str("i1"),
            SeedPart::Int(0),
        ])
        .unwrap();
        inputs.initial_state_sha256 = TrainingDriver::open(reference, rows.clone(), init)
            .unwrap()
            .checkpoint_state_sha256()
            .to_owned();
        config
            .launch_pins
            .insert("initial_state".into(), inputs.initial_state_sha256.clone());
        let mut prepared = Value::Null;
        assert!(
            execute(&args, &inputs, data, config.clone(), &Fixture, |r| {
                prepared = r;
                Ok(())
            })
            .unwrap()
        );
        let mut manifest = fs::read_to_string(&inputs.manifest.path).unwrap();
        append_entry(
            &mut manifest,
            5,
            "training_run",
            json!({"run_id":prepared["run_id"],"tripwire":prepared["tripwire"],"files":{"b1/run.json":prepared["run_json_sha256"]}}),
        );
        inputs.manifest =
            serde_json::from_value(write_pin(&run_root, "manifest", manifest.as_bytes())).unwrap();
        let mut data = inputs.load().unwrap();
        data.rows = rows.clone();
        args.prepare = false;
        let mut receipts = Vec::new();
        assert!(execute(&args, &inputs, data, config, &Fixture, |r| {
            receipts.push(r);
            Ok(())
        })
        .unwrap());
        assert_eq!(receipts[0]["checkpoint"], 1);
        assert_eq!(receipts[1]["checkpoint"], 2);
        for (update, receipt) in receipts.iter().take(2).enumerate() {
            let path = run_root.join(format!("b1/updates/update-{update:04}/checkpoint.json"));
            let checkpoint: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            assert_eq!(
                receipt["checkpoint_state_sha256"],
                checkpoint["state_sha256"]
            );
        }
        assert_ne!(
            receipts[0]["checkpoint_state_sha256"],
            receipts[1]["checkpoint_state_sha256"]
        );
        assert_eq!(receipts.last().unwrap()["event"], "exit");
        receipts
    };
    assert_eq!(run(1), run(2));
}

#[test]
fn killed_launcher_refuses_reopen_without_resume() {
    const CHILD: &str = "D8_FIXTURE_CHILD_ROOT";
    if let Some(root) = env::var_os(CHILD) {
        struct BeforeBatch(PathBuf);
        impl PilotProvider for BeforeBatch {
            fn uniform_pilot(&self) -> Arc<dyn GamePilot + Sync> {
                fs::write(self.0.join("ready"), b"").unwrap();
                loop {
                    std::thread::park();
                }
            }
            fn load_checkpoint(
                &self,
                pin: &PilotPin,
            ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
                UniformOnlyPilotProvider.load_checkpoint(pin)
            }
        }
        let root = PathBuf::from(root);
        let args = Args::parse(&[
            "run".into(),
            root.display().to_string(),
            "b1".into(),
            root.join("inputs").display().to_string(),
            "--threads".into(),
            "1".into(),
        ])
        .unwrap();
        launch_with_pilots(&args, &BeforeBatch(root)).unwrap();
        panic!("driver did not reach the first batch");
    }
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    prepare_and_authorize(&mut args, &mut spec);
    assert!(root.0.join("b1/runner.lock").is_file());
    assert!(!root.0.join("b1/runner-active.json").exists());
    let mut child = Command::new(env::current_exe().unwrap())
        .args([
            "--exact",
            "tests::killed_launcher_refuses_reopen_without_resume",
        ])
        .env(CHILD, &root.0)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while !root.0.join("ready").exists() {
        assert!(child.try_wait().unwrap().is_none());
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("child never opened D6");
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let err = launch(&args).unwrap_err();
    assert!(err.to_string().contains("stop ledger"), "{err}");
    let receipt = failure_receipt(&args, err.as_ref());
    assert_eq!(receipt["stop_kind"], "process_death");
    assert_eq!(
        receipt["checkpoint_state_sha256"],
        spec["initial_state_sha256"]
    );
    let stop: Value =
        serde_json::from_slice(&fs::read(root.0.join("b1/stops/stop-0000.json")).unwrap()).unwrap();
    assert_eq!(stop["kind"], "process_death");
    assert!(!root.0.join("b1/updates/update-0000").exists());
}
