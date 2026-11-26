//! D8 learned training. Invoke once per segment and stage:
//! `cube_training_launcher_v1 prepare|run ROOT b1|k1 INPUTS.json --threads N`.
//! For a host-loss resume, append `--resume-entry SEQ --records-repository PATH`.
//! Paths in INPUTS are relative to the working directory. Each of cube_table,
//! cube_list, ordered_pool, card_database (cube_card_ids/v1 export), fence,
//! read_plan and manifest is {"path":"...","sha256":"..."}.
//! Also supply binary_sha256, card_index_sha256, initial_state_sha256 and
//! manifest_prefix_sha256. Optional baseline contains pin (CensusBaselinePin)
//! and summary, manifest and authorization paths for full mode.
//! Preparing k1 can also supply builder_baseline for b1's earlier full-mode evidence.
//!
//! Prepare pins the manifest up to the new training_run heading. Use its exit
//! receipt to append that entry with run_id, tripwire (the run.json object)
//! and files containing the run.json path and sha256. For run, update only
//! manifest.sha256 to cover the appended bytes. Keep the original prefix pin.
//! After b1 completes, prepare k1 under the same ROOT with its new prefix.

use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::cube_draft_build_v1::card_index::{CardIndexV1, CubeTableSourceV1};
use mtg_kernel::cube_draft_build_v1::model::ModelCardRowsV1;
use mtg_kernel::cube_draft_v1::CubePool;
use mtg_kernel::cube_owner_view_v1::cube_pool_from_ordered_ids;
use mtg_kernel::cube_pod_runner_v1::{seed, PodPins, SeedPart};
use mtg_kernel::cube_read_plan_v1::{sha256_hex, shakedown_entry_values};
use mtg_kernel::cube_seat_v1::GamePilot;
use mtg_kernel::cube_training_driver_v1::{
    CensusBaselineFiles, CensusBaselinePin, PilotPin, PilotProvider, ResumeAuthorization,
    RunConfig, RunOutcome, Segment, TrainingDriver, TrainingDriverError, TripwireMode,
    UniformOnlyPilotProvider,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Duration,
};

#[path = "cube_pod_runner/worker.rs"]
mod worker;
type Fallible<T> = Result<T, String>;
#[cfg(all(test, target_arch = "x86_64"))]
#[path = "cube_training_launcher/tests.rs"]
mod tests;
type ResultBox<T> = Result<T, Box<dyn std::error::Error>>;

struct Supervised;
impl PilotProvider for Supervised {
    fn uniform_pilot(&self) -> Arc<dyn GamePilot + Sync> {
        Arc::new(worker::SupervisedUniformPilot::new(Duration::from_secs(
            mtg_kernel::cube_pod_runner_v1::DEFAULT_WATCHDOG_SECONDS,
        )))
    }
    fn load_checkpoint(
        &self,
        pin: &PilotPin,
    ) -> Result<Arc<dyn GamePilot + Sync>, TrainingDriverError> {
        UniformOnlyPilotProvider.load_checkpoint(pin)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pinned {
    path: PathBuf,
    sha256: String,
}
impl Pinned {
    fn read(&self) -> ResultBox<Vec<u8>> {
        let bytes = fs::read(&self.path)?;
        if sha256_hex(&bytes) != self.sha256 {
            return Err(format!("sha256 mismatch for {}", self.path.display()).into());
        }
        Ok(bytes)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Baseline {
    pin: CensusBaselinePin,
    manifest: PathBuf,
    summary: PathBuf,
    authorization: PathBuf,
}

impl Baseline {
    fn files(&self) -> CensusBaselineFiles {
        CensusBaselineFiles {
            manifest: self.manifest.clone(),
            summary: self.summary.clone(),
            authorization: self.authorization.clone(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inputs {
    cube_table: Pinned,
    cube_list: Pinned,
    ordered_pool: Pinned,
    card_database: Pinned,
    fence: Pinned,
    read_plan: Pinned,
    manifest: Pinned,
    manifest_prefix_sha256: String,
    card_index_sha256: String,
    binary_sha256: String,
    initial_state_sha256: String,
    baseline: Option<Baseline>,
    builder_baseline: Option<Baseline>,
}

struct Data {
    pool: CubePool,
    rows: Arc<ModelCardRowsV1>,
    table: mtg_kernel::cube_training_driver_v1::CubeTableV1,
    entries: Vec<Value>,
}

impl Inputs {
    fn load(&self) -> ResultBox<Data> {
        if env::consts::ARCH != "x86_64" {
            return Err("D0.25 requires an x86-64 shakedown host".into());
        }
        if sha256_hex(&fs::read(env::current_exe()?)?) != self.binary_sha256 {
            return Err("launcher binary sha256 mismatch".into());
        }
        let source = CubeTableSourceV1::read_v1(&self.cube_table.path, &self.cube_table.sha256)?;
        let list = self.cube_list.read()?;
        let (index, _) = CardIndexV1::provisional_v1(std::str::from_utf8(&list)?, &source)?;
        if index.pin_sha256() != self.card_index_sha256 {
            return Err("derived card index sha256 mismatch".into());
        }
        let ids: Vec<u16> = serde_json::from_slice(&self.ordered_pool.read()?)?;
        let (pool, canonical_pin) = cube_pool_from_ordered_ids(&ids)?;
        if self.ordered_pool.sha256 != canonical_pin || ids.iter().any(|&id| !index.contains(id)) {
            return Err("pool encoding or index coverage differs".into());
        }
        let db: Value = serde_json::from_slice(&self.card_database.read()?)?;
        if db["schema"] != "cube_card_ids/v1"
            || db["kernel_carddb_hash"] != format!("{KERNEL_CARDDB_HASH:016x}")
        {
            return Err("card database export differs from the compiled registry".into());
        }
        self.fence.read()?;
        self.read_plan.read()?;
        let entries = shakedown_entry_values(&self.manifest.read()?)?;
        let expected = ["d0_signed", "a6_part2_countersigned", "panel", "fixtures"];
        if !entries
            .iter()
            .take(4)
            .map(|e| e["kind"].as_str())
            .eq(expected.map(Some))
        {
            return Err("manifest must begin with signed D0, A6, panel and fixtures".into());
        }
        let table = source.bind_v1(&index)?;
        let rows = Arc::new(ModelCardRowsV1::with_registry_basics_v1(
            index.ids().to_vec(),
            table.vector_width(),
            table.scaled_rows().to_vec(),
            table.dense_columns().len(),
            table.dense_rows().to_vec(),
        )?);
        Ok(Data {
            pool,
            rows,
            table,
            entries,
        })
    }

    fn config(&self, args: &Args, data: &Data) -> ResultBox<RunConfig> {
        let segment = if args.run == "b1" {
            Segment::BuilderOnly
        } else {
            Segment::PickPhase
        };
        let mut cfg = RunConfig::shakedown(args.root.join(&args.run), &args.run, segment);
        cfg.workers = args.threads;
        cfg.pins = PodPins {
            binary: Some(self.binary_sha256.clone()),
            cube_table: Some(self.cube_table.sha256.clone()),
            cube_list: Some(self.cube_list.sha256.clone()),
            card_index: Some(self.card_index_sha256.clone()),
            fence_record: Some(self.fence.sha256.clone()),
        };
        cfg.manifest_prefix_sha256 = Some(self.manifest_prefix_sha256.clone());
        cfg.platform = Some(format!("{}-{}", env::consts::ARCH, env::consts::OS));
        cfg.launch_pins = [
            ("ordered_pool", &self.ordered_pool.sha256),
            ("card_database_export", &self.card_database.sha256),
            ("read_plan", &self.read_plan.sha256),
            ("initial_state", &self.initial_state_sha256),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.clone()))
        .collect();
        cfg.metadata.card_database_sha256 = format!("{KERNEL_CARDDB_HASH:016x}");
        cfg.metadata.column_scales = data.table.divisors().iter().map(|&v| v as u32).collect();
        cfg.metadata.dense_fields = data
            .table
            .dense_columns()
            .iter()
            .map(|f| f.token.clone())
            .collect();
        if let Some(b) = &self.baseline {
            if b.pin.build_sha256 != self.binary_sha256 || b.pin.fence_sha256 != self.fence.sha256 {
                return Err("baseline build or fence differs".into());
            }
            cfg.tripwire = TripwireMode::Full {
                baseline: b.pin.clone(),
            };
            cfg.tripwire_files = Some(b.files());
        }
        cfg.resume = args.resume.as_ref().map(|(seq, repo)| ResumeAuthorization {
            entry_seq: *seq,
            records_repository: repo.clone(),
            manifest: self.manifest.path.clone(),
            manifest_sha256: self.manifest.sha256.clone(),
        });
        Ok(cfg)
    }
}

#[derive(Clone)]
struct Args {
    prepare: bool,
    root: PathBuf,
    run: String,
    inputs: PathBuf,
    threads: usize,
    resume: Option<(u64, PathBuf)>,
}
impl Args {
    fn parse(args: &[String]) -> ResultBox<Self> {
        let [stage, root, run, inputs, threads_flag, threads, rest @ ..] = args else {
            return Err("usage: prepare|run ROOT b1|k1 INPUTS.json --threads N [--resume-entry SEQ --records-repository PATH]".into());
        };
        let threads = threads.parse()?;
        if !matches!(stage.as_str(), "prepare" | "run")
            || !matches!(run.as_str(), "b1" | "k1")
            || threads_flag != "--threads"
            || threads == 0
        {
            return Err("invalid stage, segment or thread count".into());
        }
        let resume = match rest {
            [] => None,
            [flag, seq, repo, path]
                if stage == "run" && flag == "--resume-entry" && repo == "--records-repository" =>
            {
                Some((seq.parse()?, PathBuf::from(path)))
            }
            _ => return Err("invalid resume arguments".into()),
        };
        Ok(Self {
            prepare: stage == "prepare",
            root: root.into(),
            run: run.clone(),
            inputs: inputs.into(),
            threads,
            resume,
        })
    }
}

fn training_entry<'a>(entries: &'a [Value], run: &str) -> ResultBox<Option<&'a Value>> {
    let run_id = format!("shakedown-pods/{run}");
    let mut matches = entries
        .iter()
        .filter(|e| e["kind"] == "training_run" && e["run_id"] == run_id);
    let entry = matches.next();
    if matches.next().is_some() {
        return Err("duplicate training_run entry".into());
    }
    if run == "b1" && entry.is_some_and(|e| e["seq"] != 5) {
        return Err("b1 training_run must be entry 5".into());
    }
    Ok(entry)
}

fn verify_run(config: &RunConfig, entry: &Value) -> ResultBox<()> {
    let pin = sha256_hex(&fs::read(config.root.join("run.json"))?);
    let recorded = entry["files"].as_object().is_some_and(|files| {
        files.iter().any(|(path, value)| {
            Path::new(path).file_name().is_some_and(|f| f == "run.json") && value == &pin
        })
    });
    if !recorded
        || entry["prefix_sha256"].as_str() != config.manifest_prefix_sha256.as_deref()
        || entry["tripwire"] != serde_json::to_value(&config.tripwire)?
    {
        return Err("training_run differs from run.json, prefix or tripwire".into());
    }
    Ok(())
}

fn emit(value: Value) -> ResultBox<()> {
    writeln!(io::stdout().lock(), "{value}")?;
    Ok(())
}

fn advance(
    driver: &mut TrainingDriver,
    run: &str,
    pool: &CubePool,
    pilots: &dyn PilotProvider,
    mut output: impl FnMut(Value) -> ResultBox<()>,
) -> ResultBox<bool> {
    loop {
        let outcome = driver.run_next_update(pool, pilots)?;
        let (status, update, stop) = match &outcome {
            RunOutcome::Published { update } => ("published", *update, None),
            RunOutcome::Stopped { update, kind } => ("stopped", *update, Some(kind.as_str())),
            RunOutcome::Completed => ("complete", driver.next_update(), None),
            RunOutcome::Paused { update } => ("paused", *update, None),
        };
        let mut receipt = json!({"event":"update","run_id":format!("shakedown-pods/{run}"),"update":update,"checkpoint":driver.next_update(),"checkpoint_state_sha256":driver.checkpoint_state_sha256(),"stop_kind":stop});
        if matches!(
            outcome,
            RunOutcome::Published { .. } | RunOutcome::Stopped { .. }
        ) {
            output(receipt.clone())?;
        }
        if matches!(outcome, RunOutcome::Published { .. }) {
            continue;
        }
        receipt["event"] = json!("exit");
        receipt["status"] = json!(status);
        output(receipt)?;
        return Ok(!matches!(outcome, RunOutcome::Stopped { .. }));
    }
}

fn launch(args: &Args) -> ResultBox<bool> {
    launch_with_pilots(args, &Supervised)
}

fn launch_with_pilots(args: &Args, pilots: &dyn PilotProvider) -> ResultBox<bool> {
    let inputs: Inputs = serde_json::from_slice(&fs::read(&args.inputs)?)?;
    let data = inputs.load()?;
    let config = inputs.config(args, &data)?;
    execute(args, &inputs, data, config, pilots, emit)
}

fn execute(
    args: &Args,
    inputs: &Inputs,
    data: Data,
    config: RunConfig,
    pilots: &dyn PilotProvider,
    mut output: impl FnMut(Value) -> ResultBox<()>,
) -> ResultBox<bool> {
    let entry = training_entry(&data.entries, &args.run)?;
    if args.prepare {
        if entry.is_some()
            || inputs.manifest_prefix_sha256 != inputs.manifest.sha256
            || (args.run == "b1" && data.entries.len() != 4)
        {
            return Err(
                "prepare requires the exact manifest prefix before this run's entry".into(),
            );
        }
    } else {
        verify_run(&config, entry.ok_or("missing training_run entry")?)?;
    }
    let init = seed(&[
        SeedPart::Str("shakedown-pods"),
        SeedPart::Str("pod"),
        SeedPart::Str("i1"),
        SeedPart::Int(0),
    ])?;
    let run_path = config.root.join("run.json");
    let tripwire = serde_json::to_value(&config.tripwire)?;
    let mut driver = if args.run == "k1" && args.prepare {
        let builder_entry = training_entry(&data.entries, "b1")?.ok_or("missing b1 entry")?;
        let mut previous = inputs.config(
            &Args {
                run: "b1".into(),
                resume: None,
                ..args.clone()
            },
            &data,
        )?;
        previous.manifest_prefix_sha256 = Some(
            builder_entry["prefix_sha256"]
                .as_str()
                .ok_or("missing b1 prefix")?
                .to_owned(),
        );
        previous.tripwire = serde_json::from_value(builder_entry["tripwire"].clone())?;
        if let TripwireMode::Full { baseline } = &previous.tripwire {
            let evidence = inputs
                .builder_baseline
                .as_ref()
                .or(inputs.baseline.as_ref())
                .filter(|b| &b.pin == baseline)
                .ok_or("missing evidence for b1's full-mode baseline")?;
            previous.tripwire_files = Some(evidence.files());
        }
        verify_run(&previous, builder_entry)?;
        let builder = TrainingDriver::open(previous, data.rows.clone(), init)?;
        TrainingDriver::start_pick_phase(&builder, config)?
    } else {
        TrainingDriver::open(config, data.rows, init)?
    };
    if args.run == "b1"
        && driver.next_update() == 0
        && driver.checkpoint_state_sha256() != inputs.initial_state_sha256
    {
        return Err("initial checkpoint state sha256 mismatch".into());
    }
    if args.prepare {
        if driver.next_update() != 0 {
            return Err("prepare requires checkpoint 0".into());
        }
        output(
            json!({"event":"exit","status":"prepared","run_id":format!("shakedown-pods/{}",args.run),"update":0,"checkpoint_state_sha256":driver.checkpoint_state_sha256(),"stop_kind":null,"run_json_sha256":sha256_hex(&fs::read(run_path)?),"tripwire":tripwire,"prefix_sha256":inputs.manifest_prefix_sha256}),
        )?;
        Ok(true)
    } else {
        // Bind the entry to the record while this process holds runner.lock.
        let persisted: RunConfig = serde_json::from_slice(&fs::read(&run_path)?)?;
        verify_run(
            &RunConfig {
                root: args.root.join(&args.run),
                ..persisted
            },
            entry.ok_or("missing training_run entry")?,
        )?;
        advance(&mut driver, &args.run, &data.pool, pilots, output)
    }
}

fn main() -> ExitCode {
    let argv: Vec<_> = env::args().skip(1).collect();
    if let [flag, path, interval] = argv.as_slice() {
        if flag == worker::FLAG {
            return match interval
                .parse()
                .map_err(|e| format!("{e}"))
                .and_then(|ms| worker::run_worker(path.into(), Duration::from_millis(ms)))
            {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
    }
    let args = match Args::parse(&argv) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    match launch(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            let _ = emit(failure_receipt(&args, e.as_ref()));
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn failure_receipt(args: &Args, error: &dyn std::error::Error) -> Value {
    let root = args.root.join(&args.run);
    let mut receipt = json!({"event":"exit","status":"error","error":error.to_string(),"run_id":format!("shakedown-pods/{}",args.run),"update":null,"checkpoint_state_sha256":null,"stop_kind":null});
    // D6 may have recorded the cause during open, before returning a driver.
    let Ok(files) = fs::read_dir(root.join("stops")) else {
        return receipt;
    };
    let Some(path) = files
        .flatten()
        .map(|f| f.path())
        .filter(|p| {
            p.file_name().and_then(|s| s.to_str()).is_some_and(|s| {
                s.starts_with("stop-") && s.ends_with(".json") && !s.ends_with(".resolution.json")
            })
        })
        .max()
    else {
        return receipt;
    };
    #[derive(Deserialize)]
    struct Stop {
        kind: String,
        batch_index: u64,
    }
    let stop: Option<Stop> = fs::File::open(path)
        .ok()
        .and_then(|file| serde_json::from_reader(io::BufReader::new(file)).ok());
    if let Some(stop) = stop {
        receipt["stop_kind"] = json!(stop.kind);
        receipt["update"] = json!(stop.batch_index);
        let published = |u| root.join(format!("updates/update-{u:04}/checkpoint.json"));
        let current = published(stop.batch_index);
        let checkpoint = if current.exists() {
            current
        } else {
            stop.batch_index
                .checked_sub(1)
                .map(published)
                .unwrap_or(root.join("initial/checkpoint.json"))
        };
        #[derive(Deserialize)]
        struct Header {
            state_sha256: String,
        }
        if let Some(header) = fs::File::open(checkpoint)
            .ok()
            .and_then(|f| serde_json::from_reader::<_, Header>(io::BufReader::new(f)).ok())
        {
            receipt["checkpoint_state_sha256"] = json!(header.state_sha256);
        }
    }
    receipt
}
