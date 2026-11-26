//! D8 learned training. Invoke once per segment and stage:
//! `cube_training_launcher_v1 prepare|run ROOT b<n>|k<n> INPUTS.json --threads N`.
//! For a host-loss resume, append `--resume-entry SEQ --records-repository PATH`.
//! Paths in INPUTS are relative to the working directory. Each of cube_table,
//! cube_list, ordered_pool, card_database (cube_card_ids/v1 export), fence,
//! read_plan and manifest is {"path":"...","sha256":"..."}.
//! Also supply binary_sha256, card_index_sha256, initial_state_sha256 and
//! manifest_prefix_sha256, host and build_profile (the declared provenance labels).
//! Optional baseline contains pin (CensusBaselinePin)
//! and summary, manifest and authorization paths for full mode.
//! Preparing k1 can also supply builder_baseline for b1's earlier full-mode evidence.
//!
//! Prepare pins the manifest up to the new training_run heading. Use its exit
//! receipt to append that entry with run_id, tripwire (the run.json object)
//! and files containing the run.json path and sha256. For run, update only
//! manifest.sha256 to cover the appended bytes. Keep the original prefix pin.
//! After b1 completes, prepare k1 under the same ROOT with its new prefix.

mtg_kernel::fast_global_allocator!();

use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::cube_draft_build_v1::card_index::{CardIndexV1, CubeTableSourceV1};
use mtg_kernel::cube_draft_build_v1::model::training::HeadUpdates;
use mtg_kernel::cube_draft_build_v1::model::ModelCardRowsV1;
use mtg_kernel::cube_draft_v1::{basic_land_ids, CubePool};
use mtg_kernel::cube_owner_view_v1::cube_pool_from_ordered_ids;
use mtg_kernel::cube_pod_runner_v1::{seed, PodPins, SeedPart};
use mtg_kernel::cube_read_plan_v1::{sha256_hex, shakedown_entry_values};
use mtg_kernel::cube_seat_v1::GamePilot;
use mtg_kernel::cube_training_driver_v1::{
    load_checkpoint_read_only_bytes_v1, CensusBaselineFiles, CensusBaselinePin, OpenIntent,
    PilotPin, PilotProvider, ResumeAuthorization, RunConfig, RunOutcome, Segment, TrainingDriver,
    TrainingDriverError, TripwireMode, UniformOnlyPilotProvider,
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
    host: String,
    build_profile: String,
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
    initial_checkpoint: Option<Pinned>,
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
        let list = std::str::from_utf8(&list)?;
        let (index, _) = CardIndexV1::provisional_v1(list, &source)?;
        if index.pin_sha256() != self.card_index_sha256 {
            return Err("derived card index sha256 mismatch".into());
        }
        let ids: Vec<u16> = serde_json::from_slice(&self.ordered_pool.read()?)?;
        let (pool, canonical_pin) = cube_pool_from_ordered_ids(&ids)?;
        let fence = self.fence.read()?;
        let eligible = CubePool::from_json_texts(list, "{}", Some(std::str::from_utf8(&fence)?))?;
        let basics = basic_land_ids()?;
        let mut expected: Vec<_> = eligible
            .ids()
            .iter()
            .copied()
            .filter(|id| index.contains(*id) && !basics.contains(id))
            .collect();
        expected.sort_unstable();
        if self.ordered_pool.sha256 != canonical_pin || ids != expected {
            return Err("ordered pool differs from the canonical D0.4 derivation".into());
        }
        let db: Value = serde_json::from_slice(&self.card_database.read()?)?;
        if db["schema"] != "cube_card_ids/v1"
            || db["kernel_carddb_hash"] != format!("{KERNEL_CARDDB_HASH:016x}")
        {
            return Err("card database export differs from the compiled registry".into());
        }
        let mut exported = std::collections::BTreeSet::new();
        for row in db["cards"]
            .as_array()
            .ok_or("missing card database export rows")?
        {
            let id = row["card_id"]
                .as_u64()
                .and_then(|id| u16::try_from(id).ok())
                .ok_or("invalid exported card id")?;
            if row["name"]
                .as_str()
                .and_then(mtg_kernel::card_def::card_id_by_name)
                != Some(id)
                || !exported.insert(id)
            {
                return Err("card database export differs from registry rows".into());
            }
        }
        if index.ids().iter().any(|id| !exported.contains(id)) {
            return Err("card database export omits indexed cards".into());
        }
        if !std::str::from_utf8(&self.read_plan.read()?)?.contains("mtg-kernel-cube-read-plan/v3") {
            return Err("expected the v3 D8 read plan".into());
        }
        let manifest = self.manifest.read()?;
        if !manifest.ends_with(b"\n") {
            return Err("manifest must end with LF".into());
        }
        let entries = shakedown_entry_values(&manifest)?;
        let expected = ["d0_signed", "a6_part2_countersigned", "panel", "fixtures"];
        if !entries
            .iter()
            .take(4)
            .map(|e| e["kind"].as_str())
            .eq(expected.map(Some))
        {
            return Err("manifest must begin with signed D0, A6, panel and fixtures".into());
        }
        let branch = entries[0]["lead_branch"]
            .as_str()
            .ok_or("manifest lacks lead_branch")?;
        if branch.is_empty()
            || branch == "@"
            || branch.ends_with('.')
            || branch.contains("..")
            || branch.contains("@{")
            || branch.contains(['~', '^', ':', '?', '*', '[', '\\'])
            || branch
                .split('/')
                .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
            || branch.starts_with('-')
            || branch.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err("invalid manifest lead_branch".into());
        }
        let table = source.bind_v1(&index)?;
        for baseline in self.baseline.iter().chain(&self.builder_baseline) {
            if baseline.pin.build_sha256 != self.binary_sha256
                || baseline.pin.fence_sha256 != self.fence.sha256
            {
                return Err("baseline build or fence differs".into());
            }
            baseline.pin.verify_files(&baseline.files())?;
        }
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
        let (segment, _) =
            parse_training_run(&args.run).ok_or("invalid stage, segment or thread count")?;
        let mut cfg = RunConfig::shakedown(args.root.join(&args.run), &args.run, segment);
        cfg.workers = args.threads;
        if self.host.trim().is_empty() || self.build_profile.trim().is_empty() {
            return Err("host and build_profile must be supplied".into());
        }
        cfg.host = Some(self.host.clone());
        cfg.build_profile = Some(self.build_profile.clone());
        cfg.open_intent = if args.prepare {
            OpenIntent::PrepareFresh
        } else {
            OpenIntent::Run
        };
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
        if let Some(ic) = &self.initial_checkpoint {
            cfg.launch_pins
                .insert("initial_checkpoint".to_owned(), ic.sha256.clone());
        }
        cfg.metadata.card_database_sha256 = format!("{KERNEL_CARDDB_HASH:016x}");
        cfg.metadata.column_scales = data.table.divisors().iter().map(|&v| v as u32).collect();
        cfg.metadata.dense_fields = data
            .table
            .dense_columns()
            .iter()
            .map(|f| f.token.clone())
            .collect();
        if let Some(b) = &self.baseline {
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

fn parse_training_run(run: &str) -> Option<(Segment, u64)> {
    let (segment, digits) = if let Some(rest) = run.strip_prefix('b') {
        (Segment::BuilderOnly, rest)
    } else if let Some(rest) = run.strip_prefix('k') {
        (Segment::PickPhase, rest)
    } else {
        return None;
    };
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let generation: u64 = digits.parse().ok()?;
    (generation >= 1).then_some((segment, generation))
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
            return Err("usage: prepare|run ROOT b<n>|k<n> INPUTS.json --threads N [--resume-entry SEQ --records-repository PATH]".into());
        };
        let threads = threads.parse()?;
        if !matches!(stage.as_str(), "prepare" | "run")
            || parse_training_run(run).is_none()
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

fn verify_run(config: &RunConfig, entry: &Value) -> ResultBox<RunConfig> {
    let bytes = fs::read(config.root.join("run.json"))?;
    let pin = sha256_hex(&bytes);
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
    let saved: RunConfig = serde_json::from_slice(&bytes)?;
    if saved.pins != config.pins
        || saved.launch_pins != config.launch_pins
        || saved.manifest_prefix_sha256 != config.manifest_prefix_sha256
        || saved.platform != config.platform
        || saved.build_profile != config.build_profile
        || saved.tripwire != config.tripwire
        || (config.open_intent != OpenIntent::ReadCompletedBuilder
            && (saved.host != config.host || saved.workers != config.workers))
    {
        return Err("run pin, host, worker count or build profile differs from run.json".into());
    }
    Ok(saved)
}

#[derive(Deserialize)]
struct CheckpointHeader {
    state_sha256: String,
}

fn stored_state_sha256(path: &Path) -> ResultBox<String> {
    let file = io::BufReader::new(fs::File::open(path)?);
    Ok(serde_json::from_reader::<_, CheckpointHeader>(file)?.state_sha256)
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
    let run_path = config.root.join("run.json");
    config.preflight_open()?;
    let (_, generation) =
        parse_training_run(&args.run).ok_or("invalid stage, segment or thread count")?;
    let builder_run = format!("b{generation}");
    let is_builder = config.segment == Segment::BuilderOnly;
    let mut builder_config = if is_builder {
        config.clone()
    } else {
        inputs.config(
            &Args {
                run: builder_run.clone(),
                resume: None,
                ..args.clone()
            },
            &data,
        )?
    };
    builder_config.update_range = config.update_range;
    let tripwire = serde_json::to_value(&config.tripwire)?;
    let mut driver = if generation == 1 {
        if inputs.initial_checkpoint.is_some() {
            return Err(
                "first-run segment derives checkpoint 0 from i1; omit initial_checkpoint".into(),
            );
        }
        let init = seed(&[
            SeedPart::Str("shakedown-pods"),
            SeedPart::Str("pod"),
            SeedPart::Str("i1"),
            SeedPart::Int(0),
        ])?;
        if TrainingDriver::initial_state_sha256(&builder_config, &data.rows, init)?
            != inputs.initial_state_sha256
        {
            return Err("initial checkpoint state sha256 mismatch".into());
        }
        if (!args.prepare && is_builder) || (args.prepare && !is_builder) {
            let path = builder_config.root.join("initial/checkpoint.json");
            if stored_state_sha256(&path)? != inputs.initial_state_sha256 {
                return Err("stored builder initial checkpoint state sha256 mismatch".into());
            }
        }
        if !is_builder && args.prepare {
            let builder_entry = training_entry(&data.entries, &builder_run)?
                .ok_or_else(|| format!("missing {builder_run} entry"))?;
            let mut previous = builder_config;
            previous.open_intent = OpenIntent::ReadCompletedBuilder;
            previous.manifest_prefix_sha256 = Some(
                builder_entry["prefix_sha256"]
                    .as_str()
                    .ok_or_else(|| format!("missing {builder_run} prefix"))?
                    .to_owned(),
            );
            previous.tripwire = serde_json::from_value(builder_entry["tripwire"].clone())?;
            if let TripwireMode::Full { baseline } = &previous.tripwire {
                let evidence = inputs
                    .builder_baseline
                    .as_ref()
                    .or(inputs.baseline.as_ref())
                    .filter(|b| &b.pin == baseline)
                    .ok_or_else(|| {
                        format!("missing evidence for {builder_run}'s full-mode baseline")
                    })?;
                previous.tripwire_files = Some(evidence.files());
            }
            let saved = verify_run(&previous, builder_entry)?;
            previous.workers = saved.workers;
            previous.host = saved.host;
            previous.preflight_open()?;
            let builder = TrainingDriver::open(previous, data.rows.clone(), init)?;
            TrainingDriver::start_pick_phase(&builder, config)?
        } else {
            TrainingDriver::open(config, data.rows, init)?
        }
    } else {
        // D0.12 / D0.18: a restart loads the archived pinned initial checkpoint and draws no seed again.
        let archived = inputs
            .initial_checkpoint
            .as_ref()
            .ok_or("restart run requires pinned initial_checkpoint file (D0.12 / D0.18)")?;
        let archived_bytes = archived.read()?;
        let (source_ckpt, _, _) = load_checkpoint_read_only_bytes_v1(&archived_bytes, &data.table)?;
        if source_ckpt.state_sha256 != inputs.initial_state_sha256 {
            return Err("archived initial checkpoint state sha256 mismatch".into());
        }
        if (!args.prepare && is_builder) || (args.prepare && !is_builder) {
            let stored_bytes = fs::read(builder_config.root.join("initial/checkpoint.json"))?;
            let (stored, _, _) = load_checkpoint_read_only_bytes_v1(&stored_bytes, &data.table)?;
            if stored.run != builder_run
                || stored.segment != Segment::BuilderOnly
                || stored.update != 0
                || stored.head_updates != HeadUpdates::default()
                || stored.model != source_ckpt.model
            {
                return Err(
                    "stored builder initial checkpoint differs from archived initial checkpoint"
                        .into(),
                );
            }
        }
        if is_builder {
            TrainingDriver::open_from_initial_checkpoint(
                config,
                data.rows,
                &source_ckpt,
                &inputs.initial_state_sha256,
            )?
        } else if args.prepare {
            let builder_entry = training_entry(&data.entries, &builder_run)?
                .ok_or_else(|| format!("missing {builder_run} entry"))?;
            let mut previous = builder_config;
            previous.open_intent = OpenIntent::ReadCompletedBuilder;
            previous.manifest_prefix_sha256 = Some(
                builder_entry["prefix_sha256"]
                    .as_str()
                    .ok_or_else(|| format!("missing {builder_run} prefix"))?
                    .to_owned(),
            );
            previous.tripwire = serde_json::from_value(builder_entry["tripwire"].clone())?;
            if let TripwireMode::Full { baseline } = &previous.tripwire {
                let evidence = inputs
                    .builder_baseline
                    .as_ref()
                    .or(inputs.baseline.as_ref())
                    .filter(|b| &b.pin == baseline)
                    .ok_or_else(|| {
                        format!("missing evidence for {builder_run}'s full-mode baseline")
                    })?;
                previous.tripwire_files = Some(evidence.files());
            }
            let saved = verify_run(&previous, builder_entry)?;
            previous.workers = saved.workers;
            previous.host = saved.host;
            previous.preflight_open()?;
            let builder = TrainingDriver::open_from_initial_checkpoint(
                previous,
                data.rows.clone(),
                &source_ckpt,
                &inputs.initial_state_sha256,
            )?;
            TrainingDriver::start_pick_phase(&builder, config)?
        } else {
            TrainingDriver::open_existing_pick_phase(config, data.rows)?
        }
    };
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
        if let Ok(pin) = stored_state_sha256(&checkpoint) {
            receipt["checkpoint_state_sha256"] = json!(pin);
        }
    }
    receipt
}
