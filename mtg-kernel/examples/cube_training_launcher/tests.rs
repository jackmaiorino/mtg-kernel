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
    if kind == "d0_signed" {
        body["lead_branch"] = json!("lead");
    }
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
        "host":"fixture-host","build_profile":"test",
        "cube_table":pinned(&data.join("card_semantics_v1/cube_table_v1.json")),
        "cube_list":pinned(&list),
        "ordered_pool":write_pin(root,"pool",format!("{}\n",json!(ids)).as_bytes()),
        "card_database":write_pin(root,"db",json!({"schema":"cube_card_ids/v1","kernel_carddb_hash":format!("{KERNEL_CARDDB_HASH:016x}"),"cards":ids.iter().copied().chain(basic_land_ids().unwrap()).map(|id|json!({"card_id":id,"name":mtg_kernel::card_def::CARD_DEFS[usize::from(id)].name})).collect::<Vec<_>>()}).to_string().as_bytes()),
        "fence":write_pin(root,"fence",b"{\"cards\":[]}"),
        "read_plan":write_pin(root,"read-plan",b"# mtg-kernel-cube-read-plan/v3"),
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
    for prepare in [true, false] {
        if !prepare {
            prepare_and_authorize(&mut args, &mut spec);
        }
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
            if prepare {
                assert!(!root.0.join("b1").exists(), "{field} changed the segment");
            }
        }
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
fn pool_must_equal_fenced_nonbasic_indexed_cube() {
    let root = Scratch::new();
    let (args, spec, ids) = fixture(&root.0);
    let mut stale = spec.clone();
    let name = mtg_kernel::card_def::CARD_DEFS[usize::from(ids[0])].name;
    stale["fence"] = write_pin(
        &root.0,
        "nonempty-fence",
        json!({"cards":[{"name":name,"reason":"test"}]})
            .to_string()
            .as_bytes(),
    );
    let mut basic = ids.clone();
    basic.push(basic_land_ids().unwrap()[0]);
    basic.sort_unstable();
    for bad in [
        stale,
        {
            let mut s = spec.clone();
            s["ordered_pool"] =
                write_pin(&root.0, "basic", format!("{}\n", json!(basic)).as_bytes());
            s
        },
        {
            let mut s = spec.clone();
            s["ordered_pool"] = write_pin(
                &root.0,
                "omitted",
                format!("{}\n", json!(ids[1..])).as_bytes(),
            );
            s
        },
        {
            let mut s = spec;
            s["fence"] = write_pin(&root.0, "garbage", b"not json");
            s
        },
    ] {
        fs::write(&args.inputs, bad.to_string()).unwrap();
        assert!(launch(&args).is_err());
        assert!(!root.0.join("b1").exists());
    }
}

#[test]
fn prepare_leaves_existing_or_interrupted_runs_untouched() {
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    let prefix = fs::read(root.0.join("manifest")).unwrap();
    let mut original = spec.clone();
    prepare_and_authorize(&mut args, &mut spec);
    original["manifest"] = write_pin(&root.0, "original", &prefix);
    args.prepare = true;
    fs::write(&args.inputs, original.to_string()).unwrap();
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("fresh segment"));
    let marker = root.0.join("b1/runner-active.json");
    fs::write(&marker, b"null\n").unwrap();
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("fresh segment"));
    args.run = "k1".into();
    spec["manifest_prefix_sha256"] = spec["manifest"]["sha256"].clone();
    fs::write(&args.inputs, spec.to_string()).unwrap();
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("interrupted"));
    assert_eq!(fs::read(&marker).unwrap(), b"null\n");
    fs::remove_file(marker).unwrap();
    fs::create_dir(root.0.join("b1/pending/update-0000")).unwrap();
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("interrupted"));
    assert!(!root.0.join("b1/stops/stop-0000.json").exists());
    assert!(!root.0.join("k1").exists());
}

#[test]
fn signed_manifest_header_and_worker_count_are_checked() {
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    let raw = fs::read_to_string(root.0.join("manifest")).unwrap();
    for branch in [None, Some("bad branch"), Some("bad..branch"), Some("lead")] {
        let mut manifest = String::new();
        for (i, e) in shakedown_entry_values(raw.as_bytes())
            .unwrap()
            .into_iter()
            .enumerate()
        {
            let kind = e["kind"].as_str().unwrap().to_owned();
            let mut e = e;
            e["prefix_sha256"] = json!(sha256_hex(manifest.as_bytes()));
            if i == 0 {
                e["lead_branch"] = json!(branch);
            }
            manifest.push_str(&format!(
                "## Entry {}: {kind}\n\n```json\n{e}\n```\n",
                i + 1
            ));
        }
        if branch == Some("lead") {
            manifest.pop();
        }
        let mut bad = spec.clone();
        bad["manifest"] = write_pin(&root.0, "bad-manifest", manifest.as_bytes());
        bad["manifest_prefix_sha256"] = bad["manifest"]["sha256"].clone();
        fs::write(&args.inputs, bad.to_string()).unwrap();
        assert!(launch(&args).is_err());
        assert!(!root.0.join("b1").exists());
    }
    prepare_and_authorize(&mut args, &mut spec);
    args.threads = 2;
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("worker count"));
    assert!(!root.0.join("b1/runner-active.json").exists());
}

struct DyingWorker;

// Only the duration changes. Both segments use the production pin checks,
// manifest, boundary, resume, optimizer and league paths.
fn one_update(args: &Args, spec: &Value) -> ResultBox<Vec<Value>> {
    fs::write(&args.inputs, spec.to_string())?;
    let inputs: Inputs = serde_json::from_slice(&fs::read(&args.inputs)?)?;
    let data = inputs.load()?;
    let mut config = inputs.config(args, &data)?;
    config.update_range = [0, 1];
    let mut receipts = Vec::new();
    assert!(execute(args, &inputs, data, config, &Fixture, |r| {
        receipts.push(r);
        Ok(())
    })?);
    Ok(receipts)
}

fn append_training(spec: &mut Value, seq: usize, prepared: &Value) {
    let path = PathBuf::from(spec["manifest"]["path"].as_str().unwrap());
    let mut manifest = fs::read_to_string(&path).unwrap();
    append_entry(
        &mut manifest,
        seq,
        "training_run",
        json!({"run_id":prepared["run_id"],"tripwire":prepared["tripwire"],"files":{"run.json":prepared["run_json_sha256"]}}),
    );
    fs::write(&path, manifest).unwrap();
    spec["manifest"] = pinned(&path);
}

fn baseline(root: &Path, spec: &Value) -> Value {
    let summary=write_pin(root,"summary",br#"{"games":1,"completed":1,"halted":0,"stop_kinds":[],"cards":[],"order_errors":[],"stop_kind_mismatches":[]}"#);
    let auth = write_pin(root, "authorization", b"fixture authorization");
    let mut pin = json!({"run":"census-b","build_sha256":spec["binary_sha256"],"fence_sha256":spec["fence"]["sha256"],"summary_sha256":summary["sha256"],"authorization_sha256":auth["sha256"]});
    let manifest = write_pin(root, "baseline-manifest", pin.to_string().as_bytes());
    pin["manifest_sha256"] = manifest["sha256"].clone();
    json!({"pin":pin,"summary":summary["path"],"manifest":manifest["path"],"authorization":auth["path"]})
}

#[test]
fn k1_prepares_from_completed_full_mode_builder_and_runs() {
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    spec["baseline"] = baseline(&root.0, &spec);
    let mut bad = spec.clone();
    bad["baseline"]["pin"]["manifest_sha256"] = json!("0".repeat(64));
    assert!(one_update(&args, &bad).is_err());
    assert!(!root.0.join("b1").exists());
    let prepared = one_update(&args, &spec).unwrap();
    assert_eq!(prepared[0]["tripwire"]["mode"], "full");
    append_training(&mut spec, 5, &prepared[0]);
    args.prepare = false;
    let end = one_update(&args, &spec).unwrap().pop().unwrap();
    args.prepare = true;
    args.run = "k1".into();
    spec["manifest_prefix_sha256"] = spec["manifest"]["sha256"].clone();
    spec["builder_baseline"] = spec.as_object_mut().unwrap().remove("baseline").unwrap();
    let inputs: Inputs = serde_json::from_value(spec.clone()).unwrap();
    let data = inputs.load().unwrap();
    let mut different_platform = inputs.config(&args, &data).unwrap();
    different_platform.platform = Some("other-platform".into());
    different_platform.update_range = [0, 1];
    assert!(
        execute(&args, &inputs, data, different_platform, &Fixture, |_| Ok(
            ()
        ))
        .unwrap_err()
        .to_string()
        .contains("segment boundary")
    );
    assert!(!root.0.join("k1").exists());
    let pick = one_update(&args, &spec).unwrap();
    assert_eq!(pick[0]["tripwire"]["mode"], "q9");
    let record: Value =
        serde_json::from_slice(&fs::read(root.0.join("k1/run.json")).unwrap()).unwrap();
    assert_eq!(
        record["boundary"]["builder_final_state_sha256"],
        end["checkpoint_state_sha256"]
    );
    assert_eq!(record["host"], "fixture-host");
    assert_eq!(record["build_profile"], "test");
    append_training(&mut spec, 6, &pick[0]);
    args.prepare = false;
    let end = one_update(&args, &spec).unwrap().pop().unwrap();
    assert_eq!(end["status"], "complete");
    assert_eq!(end["checkpoint"], 1);
}

fn fixture_git(repo: &Path, args: &[&str]) {
    let result = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@invalid.test",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn resume_cli_accepts_lead_entry_and_refuses_merge_introduction() {
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    let prepared = one_update(&args, &spec).unwrap();
    append_training(&mut spec, 5, &prepared[0]);
    let prefix = fs::read_to_string(root.0.join("manifest")).unwrap();
    fs::write(root.0.join("b1/runner-active.json"), b"null\n").unwrap();
    for merged in [true, false] {
        let repo = root.0.join(if merged { "merged" } else { "direct" });
        let path = repo
            .join("LANES/lead/reviews/cube-briefs-20261122/decl-20261124/d0/SHAKEDOWN-MANIFEST.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fixture_git(&repo, &["init", "-q", "-b", "lead"]);
        fs::write(&path, &prefix).unwrap();
        fixture_git(&repo, &["add", "."]);
        fixture_git(&repo, &["commit", "-qm", "Prepared run"]);
        if merged {
            fixture_git(&repo, &["switch", "-qc", "other"]);
        }
        let mut manifest = prefix.clone();
        append_entry(
            &mut manifest,
            6,
            "resume",
            json!({"run_id":"shakedown-pods/b1","host_loss_evidence":"fixture boot record","last_published_update":0,"resume_batch":0}),
        );
        fs::write(&path, &manifest).unwrap();
        fixture_git(&repo, &["add", "."]);
        fixture_git(&repo, &["commit", "-qm", "Record host loss"]);
        if merged {
            fixture_git(&repo, &["switch", "-q", "lead"]);
            fixture_git(&repo, &["merge", "--no-ff", "-qm", "Merge record", "other"]);
        }
        spec["manifest"] = pinned(&path);
        args = Args::parse(&[
            "run".into(),
            root.0.display().to_string(),
            "b1".into(),
            args.inputs.display().to_string(),
            "--threads".into(),
            "1".into(),
            "--resume-entry".into(),
            "6".into(),
            "--records-repository".into(),
            repo.display().to_string(),
        ])
        .unwrap();
        let result = one_update(&args, &spec);
        if merged {
            assert!(result.unwrap_err().to_string().contains("non-merge"));
            assert!(root.0.join("b1/runner-active.json").exists());
            assert!(!root.0.join("b1/stops/stop-0000.json").exists());
        } else {
            assert_eq!(result.unwrap().last().unwrap()["checkpoint"], 1);
            assert!(root
                .0
                .join("b1/resumes/entry-00000000000000000006.json")
                .is_file());
            assert!(!root.0.join("b1/runner-active.json").exists());
        }
    }
}

impl PilotProvider for DyingWorker {
    fn uniform_pilot(&self) -> Arc<dyn GamePilot + Sync> {
        let mut pilot = worker::SupervisedUniformPilot::new(Duration::from_secs(600));
        pilot.worker_command = Some(|| {
            let mut cmd = Command::new(if cfg!(windows) { "python" } else { "python3" });
            cmd.args(["-c", "import sys; sys.stdin.readline(); sys.exit(37)"]);
            cmd
        });
        Arc::new(pilot)
    }
    fn load_checkpoint(
        &self,
        pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        UniformOnlyPilotProvider.load_checkpoint(pin)
    }
}

#[test]
fn attempt_child_exit_fires_process_death_at_the_failed_game() {
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    prepare_and_authorize(&mut args, &mut spec);
    assert!(!launch_with_pilots(&args, &DyingWorker).unwrap());
    let stop: Value =
        serde_json::from_slice(&fs::read(root.0.join("b1/stops/stop-0000.json")).unwrap()).unwrap();
    assert_eq!(stop["kind"], "process_death");
    assert_eq!(stop["pod"], 0);
    assert_eq!(stop["game"], 0);
    assert!(stop["process_death"]["found_unix_ms"].is_number());
    assert_eq!(stop["firing"]["rule"]["target"], "process_death");
    let exit = stop["error"].as_str().unwrap();
    assert!(
        exit.contains("pid ") && exit.contains("37") && exit.contains("attempt 0"),
        "{exit}"
    );
    assert!(!root.0.join("b1/updates/update-0000").exists());
}

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
        config.open_intent = OpenIntent::Run;
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
        // Reference checkpoints from this fixture with the system allocator.
        assert_eq!(
            receipts[0]["checkpoint_state_sha256"],
            "03a641b0421c2ee9d1f21b5ec34388285d0f30e3a7c98b9fc6967c15316b18ed"
        );
        assert_eq!(
            receipts[1]["checkpoint_state_sha256"],
            "7e567b34f15b15f8c7b1bf836ab35a23f9de52ecd9871b6a56a59cdadd25ae0f"
        );
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

#[test]
fn restart_run_ids_parse_and_prepare_after_stopped_b1() {
    assert_eq!(parse_training_run("b1"), Some((Segment::BuilderOnly, 1)));
    assert_eq!(parse_training_run("b2"), Some((Segment::BuilderOnly, 2)));
    assert_eq!(parse_training_run("k1"), Some((Segment::PickPhase, 1)));
    assert_eq!(parse_training_run("k2"), Some((Segment::PickPhase, 2)));
    for bad in ["", "b", "k", "b0", "k0", "b01", "k02", "i1", "b-1", "x2"] {
        assert_eq!(parse_training_run(bad), None, "{bad}");
    }

    // Prepare b1 first to produce the archived pinned initial checkpoint (state sha256 90cff6d7...).
    let root = Scratch::new();
    let (mut args, mut spec, _) = fixture(&root.0);
    fs::write(&args.inputs, spec.to_string()).unwrap();
    assert!(launch(&args).unwrap());
    let b1_init_path = root.0.join("b1/initial/checkpoint.json");
    let b1_init_pin = pinned(&b1_init_path);
    let b1_ckpt: Value = serde_json::from_slice(&fs::read(&b1_init_path).unwrap()).unwrap();
    assert_eq!(
        b1_ckpt["state_sha256"],
        "90cff6d7f7c58ad606c71d14274457a2cbe2836e2113e192d8012eec9fd2016f"
    );

    let b1_run_pin = pinned(&root.0.join("b1/run.json"))["sha256"].clone();
    let mut manifest = fs::read_to_string(root.0.join("manifest")).unwrap();
    append_entry(
        &mut manifest,
        5,
        "training_run",
        json!({"run_id":"shakedown-pods/b1","tripwire":{"mode":"q9"},"files":{"b1/run.json":b1_run_pin}}),
    );
    append_entry(
        &mut manifest,
        6,
        "stop",
        json!({"run_id":"shakedown-pods/b1","stop_kind":"tripwire"}),
    );
    spec["manifest"] = write_pin(&root.0, "manifest", manifest.as_bytes());
    spec["manifest_prefix_sha256"] = spec["manifest"]["sha256"].clone();

    // Restart b2 refuses without pinned initial_checkpoint (D0.12 / D0.18).
    args.run = "b2".into();
    args.prepare = true;
    fs::write(&args.inputs, spec.to_string()).unwrap();
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("initial_checkpoint"));
    assert!(!root.0.join("b2").exists());

    // Restart b2 refuses if initial_state_sha256 does not match the archived file's signed state digest.
    let mut wrong_pin = spec.clone();
    wrong_pin["initial_checkpoint"] = b1_init_pin.clone();
    wrong_pin["initial_state_sha256"] = json!("0".repeat(64));
    fs::write(&args.inputs, wrong_pin.to_string()).unwrap();
    assert!(launch(&args)
        .unwrap_err()
        .to_string()
        .contains("state sha256 mismatch"));
    assert!(!root.0.join("b2").exists());

    // Restart b2 loads the archived b1 initial checkpoint under its signed digest 90cff6d7... with no seed draw.
    spec["initial_checkpoint"] = b1_init_pin.clone();
    fs::write(&args.inputs, spec.to_string()).unwrap();
    assert!(launch(&args).unwrap());
    assert!(root.0.join("b2/run.json").is_file());
    assert!(!root.0.join("b2/updates/update-0000").exists());
    let b2_ckpt: Value =
        serde_json::from_slice(&fs::read(root.0.join("b2/initial/checkpoint.json")).unwrap())
            .unwrap();
    assert_eq!(b2_ckpt["run"], "b2");
    assert_eq!(b2_ckpt["model"], b1_ckpt["model"]);
    let b2_run_doc: Value =
        serde_json::from_slice(&fs::read(root.0.join("b2/run.json")).unwrap()).unwrap();
    assert_eq!(
        b2_run_doc["launch_pins"]["initial_state"],
        "90cff6d7f7c58ad606c71d14274457a2cbe2836e2113e192d8012eec9fd2016f"
    );
    assert_eq!(
        b2_run_doc["launch_pins"]["initial_checkpoint"],
        b1_init_pin["sha256"]
    );

    // Authorizing b2 at seq 7 passes verify_run; tampering with b2/initial/checkpoint.json
    // (e.g. empty league with a valid recomputed canonical state_sha256) must be refused by
    // both b2 run (keeping b2's pre-entry-7 prefix) and k2 prepare, as well as direct driver open.
    let b2_prefix = spec["manifest_prefix_sha256"].clone();
    let b2_pin = pinned(&root.0.join("b2/run.json"))["sha256"].clone();
    append_entry(
        &mut manifest,
        7,
        "training_run",
        json!({"run_id":"shakedown-pods/b2","tripwire":{"mode":"q9"},"files":{"b2/run.json":b2_pin}}),
    );
    spec["manifest"] = write_pin(&root.0, "manifest", manifest.as_bytes());

    let b2_init_path = root.0.join("b2/initial/checkpoint.json");
    let b2_init_raw = fs::read(&b2_init_path).unwrap();
    let mut tampered: mtg_kernel::cube_training_driver_v1::Checkpoint =
        serde_json::from_slice(&b2_init_raw).unwrap();
    tampered.league.clear();
    tampered.state_sha256.clear();
    let mut pin_bytes = b"mtg-kernel-cube-checkpoint-state/v1\0".to_vec();
    serde_json::to_writer(&mut pin_bytes, &tampered).unwrap();
    tampered.state_sha256 = sha256_hex(&pin_bytes);
    fs::write(&b2_init_path, serde_json::to_vec(&tampered).unwrap()).unwrap();

    spec["manifest_prefix_sha256"] = b2_prefix;
    fs::write(&args.inputs, spec.to_string()).unwrap();
    args.run = "b2".into();
    args.prepare = false;
    let err = launch(&args).unwrap_err().to_string();
    assert!(err.contains("state sha256 mismatch"), "{err}");
    assert!(!root.0.join("b2/updates/update-0000").exists());

    // Direct driver open under lock also rejects the tampered staged checkpoint 0.
    let inputs: Inputs = serde_json::from_slice(&fs::read(&args.inputs).unwrap()).unwrap();
    let data = inputs.load().unwrap();
    let cfg = inputs.config(&args, &data).unwrap();
    let (src_ckpt, _, _) = load_checkpoint_read_only_bytes_v1(
        &inputs.initial_checkpoint.as_ref().unwrap().read().unwrap(),
        &data.table,
    )
    .unwrap();
    let drv_err = TrainingDriver::open_from_initial_checkpoint(
        cfg,
        data.rows,
        &src_ckpt,
        &inputs.initial_state_sha256,
    )
    .err()
    .unwrap()
    .to_string();
    assert!(
        drv_err.contains("expected canonical checkpoint 0"),
        "{drv_err}"
    );

    spec["manifest_prefix_sha256"] = spec["manifest"]["sha256"].clone();
    fs::write(&args.inputs, spec.to_string()).unwrap();
    args.run = "k2".into();
    args.prepare = true;
    let err = launch(&args).unwrap_err().to_string();
    assert!(err.contains("state sha256 mismatch"), "{err}");
    assert!(!root.0.join("k2/run.json").exists());

    // Restore valid staged b2 checkpoint 0; k2 prepare now rejects only because b2 is incomplete.
    fs::write(&b2_init_path, &b2_init_raw).unwrap();
    let err = launch(&args).unwrap_err().to_string();
    assert!(err.contains("completed builder segment"), "{err}");
    assert!(!root.0.join("k2/run.json").exists());
}
