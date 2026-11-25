//! Cube pod runner CLI (`mtg-kernel-cube-pod-record/v2` and
//! `mtg-kernel-cube-read-plan/v1`).
//!
//! Modes (`eternal-fixtures` build):
//! 1. Training / client pods:
//!    `cube_pod_runner_v1 --namespace NS --run RUN --pods N [--pod-start J]`
//!    `[--use training|shakedown|test|client] [--older 6,7]`
//!    `[--workers W] [--watchdog-seconds 600]`
//!    `[--cube PATH] [--gaps PATH] [--fence PATH]`
//!    `[--out PATH] [--report PATH]`
//! 2. Prep set (`READ-PLAN-v1` section 4-5, drafts and builds without playing):
//!    `cube_pod_runner_v1 --prep-set DIR --execution E --declaration PATH`
//!    `--binary-pin SHA --card-index-pin SHA --cube-table-pin SHA`
//!    `[--panel-pin SHA] [--platform ID] [--host HOST]`
//!    `[--cube PATH] [--gaps PATH] [--fence PATH]`
//! 3. Supervised plan set (`READ-PLAN-v1` section 5-6, writes `exec-<E>/`):
//!    `cube_pod_runner_v1 --plan-set DIR --execution E --declaration PATH`
//!    `--binary-pin SHA --card-index-pin SHA --cube-table-pin SHA`
//!    `[--pool-pin SHA] [--constants-pin SHA] [--panel-pin SHA]`
//!    `[--platform ID] [--host HOST] [--workers W]`
//!    `[--cube PATH] [--gaps PATH] [--fence PATH]`
//! 4. Single-pod worker (spawned by the supervisor for `--plan-set`):
//!    `cube_pod_runner_v1 --worker-plan PLAN --out RECORD --timing TIMING`
//!    `--journal JOURNAL --execution E [--host HOST] [--workers W]`
//!    `[--cube PATH] [--gaps PATH] [--fence PATH]`
mtg_kernel::fast_global_allocator!();

use mtg_kernel::cube_draft_v1::CubePool;
use mtg_kernel::cube_pod_runner_v1::{
    run_training_pod, run_uniform_training_batch, training_roles_with_older,
    uniform_training_chairs, BatchReport, PilotInfo, PodCaps, PodPins, PodUse, TrainingPodConfig,
    UniformBatchConfig, DEFAULT_WATCHDOG_SECONDS,
};
use mtg_kernel::cube_read_plan_v1::{
    execute_read_plan_v1, parse_and_validate_read_plan_json, run_prep_set_execution, sha256_hex,
    supervise_plan_set_execution, to_canonical_compact_json, to_canonical_pretty_json,
    CommandPodWorkerLauncher, PodProgressJournal, ReadPlanRunOptions, RuntimePins,
    RuntimeVerification, UniformSeatFactory,
};
use mtg_kernel::cube_seat_v1::UniformPilot;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

type Fallible<T> = Result<T, String>;

const VALUE_FLAGS: &[&str] = &[
    "--namespace",
    "--run",
    "--pods",
    "--pod-start",
    "--use",
    "--older",
    "--workers",
    "--watchdog-seconds",
    "--cube",
    "--gaps",
    "--fence",
    "--out",
    "--report",
    "--prep-set",
    "--plan-set",
    "--worker-plan",
    "--timing",
    "--journal",
    "--execution",
    "--declaration",
    "--binary-pin",
    "--card-index-pin",
    "--cube-table-pin",
    "--pool-pin",
    "--constants-pin",
    "--panel-pin",
    "--platform",
    "--host",
];

struct Args {
    values: BTreeMap<String, String>,
}

impl Args {
    fn parse(raw: Vec<String>) -> Fallible<Self> {
        let mut values = BTreeMap::new();
        let mut items = raw.into_iter();
        while let Some(name) = items.next() {
            if !VALUE_FLAGS.contains(&name.as_str()) {
                return Err(format!("unknown argument {name:?}"));
            }
            let value = items
                .next()
                .ok_or_else(|| format!("{name} requires a value"))?;
            if values.insert(name.clone(), value).is_some() {
                return Err(format!("{name} specified twice"));
            }
        }
        Ok(Args { values })
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn required(&self, name: &str) -> Fallible<&str> {
        self.get(name).ok_or_else(|| format!("{name} is required"))
    }

    fn number(&self, name: &str) -> Fallible<Option<u64>> {
        self.get(name)
            .map(|text| {
                text.parse::<u64>()
                    .map_err(|error| format!("{name} {text:?}: {error}"))
            })
            .transpose()
    }
}

fn parse_use(text: &str) -> Fallible<PodUse> {
    match text {
        "training" => Ok(PodUse::Training),
        "shakedown" => Ok(PodUse::Shakedown),
        "dose" => Ok(PodUse::Dose),
        "read_a" => Ok(PodUse::ReadA),
        "read_b" => Ok(PodUse::ReadB),
        "paired_build" => Ok(PodUse::PairedBuild),
        "sizing" => Ok(PodUse::Sizing),
        "test" => Ok(PodUse::Test),
        "client" => Ok(PodUse::Client),
        other => Err(format!("unknown --use {other:?}")),
    }
}

fn parse_older(text: &str) -> Fallible<[usize; 2]> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 2 {
        return Err(format!(
            "--older {text:?}: expected two comma-separated chair indices"
        ));
    }
    let a = parts[0]
        .parse::<usize>()
        .map_err(|error| format!("--older {text:?}: {error}"))?;
    let b = parts[1]
        .parse::<usize>()
        .map_err(|error| format!("--older {text:?}: {error}"))?;
    Ok([a, b])
}

fn read_bytes(path: &str) -> Fallible<Vec<u8>> {
    std::fs::read(path).map_err(|error| format!("{path}: {error}"))
}

fn utf8(path: &str, bytes: &[u8]) -> Fallible<String> {
    String::from_utf8(bytes.to_vec()).map_err(|error| format!("{path}: {error}"))
}

struct LoadedPool {
    pool: CubePool,
    cube_path: String,
    gaps_path: String,
    fence_path: Option<String>,
    cube_sha256: String,
    fence_sha256: Option<String>,
}

fn load_pool(args: &Args) -> Fallible<LoadedPool> {
    let cube_path = args
        .get("--cube")
        .unwrap_or("data/cube_v1/mtgo_vintage_cube_20261007.json")
        .to_owned();
    let gaps_path = args
        .get("--gaps")
        .unwrap_or("data/cube_v1/mtgo_vintage_cube_20261007_gaps.json")
        .to_owned();
    let fence_path = args.get("--fence").map(str::to_owned);
    let cube_bytes = read_bytes(&cube_path)?;
    let gaps_bytes = read_bytes(&gaps_path)?;
    let fence_bytes = fence_path.as_deref().map(read_bytes).transpose()?;
    let fence_text = match (fence_path.as_deref(), &fence_bytes) {
        (Some(path), Some(bytes)) => Some(utf8(path, bytes)?),
        _ => None,
    };
    let pool = CubePool::from_json_texts(
        &utf8(&cube_path, &cube_bytes)?,
        &utf8(&gaps_path, &gaps_bytes)?,
        fence_text.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    Ok(LoadedPool {
        pool,
        cube_path,
        gaps_path,
        fence_path,
        cube_sha256: sha256_hex(&cube_bytes),
        fence_sha256: fence_bytes.as_deref().map(sha256_hex),
    })
}

fn load_verification(args: &Args, loaded: &LoadedPool) -> Fallible<RuntimeVerification> {
    let binary = match args.get("--binary-pin") {
        Some(pin) => pin.to_owned(),
        None => {
            let exe = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
            let bytes =
                std::fs::read(&exe).map_err(|error| format!("{}: {error}", exe.display()))?;
            sha256_hex(&bytes)
        }
    };
    let fence_record = loaded
        .fence_sha256
        .clone()
        .ok_or_else(|| "--fence is required for read-plan/prep pin verification".to_owned())?;
    let card_index = args.required("--card-index-pin")?.to_owned();
    let cube_table = args.required("--cube-table-pin")?.to_owned();
    let declaration_path = args.required("--declaration")?;
    let declaration_bytes = read_bytes(declaration_path)?;
    let platform = args.get("--platform").unwrap_or("local").to_owned();
    let host = args.get("--host").unwrap_or("local").to_owned();
    Ok(RuntimeVerification {
        pins: RuntimePins {
            binary,
            cube_list: loaded.cube_sha256.clone(),
            fence_record,
            card_index,
            cube_table,
            pool_sha256: args.get("--pool-pin").map(str::to_owned),
            constants_sha256: args.get("--constants-pin").map(str::to_owned),
        },
        platform,
        host,
        declaration_bytes,
        pinned_panel_sha256: args.get("--panel-pin").map(str::to_owned),
    })
}

fn run_worker_plan(args: &Args, loaded: &LoadedPool) -> Fallible<()> {
    let plan_path = args.required("--worker-plan")?;
    let out_path = PathBuf::from(args.required("--out")?);
    let timing_path = PathBuf::from(args.required("--timing")?);
    let journal_path = PathBuf::from(args.required("--journal")?);
    let execution = args
        .number("--execution")?
        .ok_or("--execution is required")?;
    let workers = args.number("--workers")?.unwrap_or(1).max(1) as usize;
    let host = args.get("--host").unwrap_or("local");

    let plan_text =
        std::fs::read_to_string(plan_path).map_err(|error| format!("{plan_path}: {error}"))?;
    let plan = parse_and_validate_read_plan_json(&plan_text).map_err(|e| e.to_string())?;
    let journal = PodProgressJournal::create(journal_path).map_err(|e| e.to_string())?;
    let run_opts = ReadPlanRunOptions {
        execution,
        workers,
        host,
        watchdog_override: None,
        journal: Some(&journal),
    };
    let output = execute_read_plan_v1(
        &loaded.pool,
        &plan,
        &UniformSeatFactory,
        &UniformPilot,
        &run_opts,
    )
    .map_err(|e| e.to_string())?;

    let record_text = to_canonical_pretty_json(&output.record).map_err(|e| e.to_string())?;
    let mut rec_file =
        File::create_new(&out_path).map_err(|e| format!("{}: {e}", out_path.display()))?;
    rec_file
        .write_all(record_text.as_bytes())
        .map_err(|e| format!("{}: {e}", out_path.display()))?;
    rec_file
        .sync_all()
        .map_err(|e| format!("{}: {e}", out_path.display()))?;

    let mut timing_file =
        File::create_new(&timing_path).map_err(|e| format!("{}: {e}", timing_path.display()))?;
    for entry in &output.timings {
        let line = to_canonical_compact_json(entry).map_err(|e| e.to_string())?;
        writeln!(timing_file, "{line}").map_err(|e| format!("{}: {e}", timing_path.display()))?;
    }
    timing_file
        .sync_all()
        .map_err(|e| format!("{}: {e}", timing_path.display()))?;
    Ok(())
}

fn run_cli(args: &Args) -> Fallible<()> {
    let loaded = load_pool(args)?;

    if args.get("--worker-plan").is_some() {
        return run_worker_plan(args, &loaded);
    }

    if let Some(prep_dir) = args.get("--prep-set") {
        let execution = args
            .number("--execution")?
            .ok_or("--execution is required with --prep-set")?;
        let verification = load_verification(args, &loaded)?;
        let receipt = run_prep_set_execution(
            &loaded.pool,
            Path::new(prep_dir),
            execution,
            &UniformSeatFactory,
            &verification,
            None,
        )
        .map_err(|error| error.to_string())?;
        let text = to_canonical_pretty_json(&receipt).map_err(|e| e.to_string())?;
        print!("{text}");
        return Ok(());
    }

    if let Some(plan_dir) = args.get("--plan-set") {
        let execution = args
            .number("--execution")?
            .ok_or("--execution is required with --plan-set")?;
        let workers = args.number("--workers")?.unwrap_or(1).max(1) as usize;
        let verification = load_verification(args, &loaded)?;
        let exe = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
        let cube_path = loaded.cube_path.clone();
        let gaps_path = loaded.gaps_path.clone();
        let fence_path = loaded.fence_path.clone();
        let host = verification.host.clone();
        let launcher = CommandPodWorkerLauncher::new(
            move |plan_path: &Path,
                  record_path: &Path,
                  timing_path: &Path,
                  journal_path: &Path,
                  exec_num: u64| {
                let mut cmd = Command::new(&exe);
                cmd.arg("--worker-plan")
                    .arg(plan_path)
                    .arg("--out")
                    .arg(record_path)
                    .arg("--timing")
                    .arg(timing_path)
                    .arg("--journal")
                    .arg(journal_path)
                    .arg("--execution")
                    .arg(exec_num.to_string())
                    .arg("--workers")
                    .arg(workers.to_string())
                    .arg("--host")
                    .arg(&host)
                    .arg("--cube")
                    .arg(&cube_path)
                    .arg("--gaps")
                    .arg(&gaps_path);
                if let Some(ref fp) = fence_path {
                    cmd.arg("--fence").arg(fp);
                }
                cmd
            },
        );
        let receipt = supervise_plan_set_execution(
            &loaded.pool,
            Path::new(plan_dir),
            execution,
            &UniformSeatFactory,
            &verification,
            &launcher,
        )
        .map_err(|error| error.to_string())?;
        let text = to_canonical_pretty_json(&receipt).map_err(|e| e.to_string())?;
        print!("{text}");
        return Ok(());
    }

    // Default mode: training or client pod batch.
    let namespace = args.required("--namespace")?.to_owned();
    let run = args.required("--run")?.to_owned();
    let pods = args.number("--pods")?.ok_or("--pods is required")?;
    if pods == 0 {
        return Err("--pods must be at least 1".to_owned());
    }
    let pod_start = args.number("--pod-start")?.unwrap_or(0);
    let pod_use = parse_use(args.get("--use").unwrap_or("training"))?;
    let older = parse_older(args.get("--older").unwrap_or("6,7"))?;
    let roles = training_roles_with_older(older).map_err(|error| error.to_string())?;
    let workers = args.number("--workers")?.unwrap_or(1).max(1) as usize;
    let watchdog_seconds = args
        .number("--watchdog-seconds")?
        .unwrap_or(DEFAULT_WATCHDOG_SECONDS);

    let out_path = args.get("--out").map(PathBuf::from);
    if let Some(path) = out_path.as_ref().filter(|path| path.exists()) {
        return Err(format!(
            "{} already exists; outputs are created fresh",
            path.display()
        ));
    }
    let report_path = args.get("--report").map(PathBuf::from);
    if let Some(path) = report_path.as_ref().filter(|path| path.exists()) {
        return Err(format!(
            "{} already exists; outputs are created fresh",
            path.display()
        ));
    }

    let pins = PodPins {
        binary: None,
        cube_list: Some(loaded.cube_sha256),
        fence_record: loaded.fence_sha256,
        card_index: None,
        cube_table: None,
    };

    let mut caps = PodCaps::training_a2();
    caps.watchdog_seconds = watchdog_seconds;

    let pod_end = pod_start
        .checked_add(pods)
        .ok_or("--pod-start + --pods overflows u64")?;

    let records = if pod_use == PodUse::Client {
        let mut out = Vec::with_capacity(pods as usize);
        for pod_index in pod_start..pod_end {
            let pod_cfg = TrainingPodConfig {
                pod_use,
                namespace: namespace.clone(),
                run: run.clone(),
                pod_index,
                pins: pins.clone(),
                pilot_info: PilotInfo::uniform(),
                caps,
                workers,
            };
            let mut chairs = uniform_training_chairs(roles);
            let rec = run_training_pod(&loaded.pool, &pod_cfg, &mut chairs, &UniformPilot)
                .map_err(|error| error.to_string())?;
            out.push(rec);
        }
        out
    } else {
        let batch_config = UniformBatchConfig {
            pod_use,
            namespace,
            run,
            pod_indices: (pod_start..pod_end).collect(),
            roles,
            pins,
            pilot_info: PilotInfo::uniform(),
            caps,
            workers,
        };
        run_uniform_training_batch(&loaded.pool, &batch_config, &UniformPilot)
            .map_err(|error| error.to_string())?
    };

    let report = BatchReport::from_records(&records);

    match out_path {
        Some(path) => {
            let file =
                File::create_new(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            let mut writer = BufWriter::new(file);
            for record in &records {
                writeln!(writer, "{}", record.to_json_line())
                    .map_err(|error| format!("{}: {error}", path.display()))?;
            }
            writer
                .flush()
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        None => {
            let stdout = io::stdout();
            let mut writer = BufWriter::new(stdout.lock());
            for record in &records {
                writeln!(writer, "{}", record.to_json_line())
                    .map_err(|error| format!("stdout: {error}"))?;
            }
            writer.flush().map_err(|error| format!("stdout: {error}"))?;
        }
    }

    let report_json = serde_json::to_string_pretty(&report).expect("a batch report is plain data");
    if let Some(path) = report_path {
        let mut file =
            File::create_new(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        writeln!(file, "{report_json}").map_err(|error| format!("{}: {error}", path.display()))?;
        file.flush()
            .map_err(|error| format!("{}: {error}", path.display()))?;
    } else {
        eprintln!("{report_json}");
    }

    Ok(())
}

fn main() {
    let result = Args::parse(std::env::args().skip(1).collect()).and_then(|args| run_cli(&args));
    if let Err(error) = result {
        eprintln!("cube_pod_runner_v1: {error}");
        std::process::exit(2);
    }
}
