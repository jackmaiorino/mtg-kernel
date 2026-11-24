//! Runs N uniform training pods in a namespace and run and writes one
//! `mtg-kernel-cube-pod-record/v1` JSON line per pod.
//!
//! If any pod stops (exhausts its 3 retries) or trips the per-pod watchdog,
//! nothing is published from the batch and the process exits non-zero.
//!
//! Usage (`eternal-fixtures` build):
//!   cube_pod_runner_v1 --namespace NS --run RUN --pods N
//!                      [--pod-start J] [--use training] [--older 6,7]
//!                      [--workers W] [--watchdog-seconds 600]
//!                      [--cube PATH] [--gaps PATH] [--fence PATH]
//!                      [--out PATH] [--report PATH]
mtg_kernel::fast_global_allocator!();

use mtg_kernel::cube_draft_v1::CubePool;
use mtg_kernel::cube_pod_runner_v1::{
    run_uniform_training_batch, training_roles_with_older, BatchReport, PilotInfo, PodCaps,
    PodPins, PodUse, UniformBatchConfig, DEFAULT_WATCHDOG_SECONDS,
};
use mtg_kernel::cube_seat_v1::UniformPilot;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

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

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn run_cli(args: &Args) -> Fallible<()> {
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

    let cube_path = args
        .get("--cube")
        .unwrap_or("data/cube_v1/mtgo_vintage_cube_20261007.json");
    let gaps_path = args
        .get("--gaps")
        .unwrap_or("data/cube_v1/mtgo_vintage_cube_20261007_gaps.json");
    let cube_bytes = read_bytes(cube_path)?;
    let gaps_bytes = read_bytes(gaps_path)?;
    let fence_bytes = args.get("--fence").map(read_bytes).transpose()?;
    let fence_text = match (args.get("--fence"), &fence_bytes) {
        (Some(path), Some(bytes)) => Some(utf8(path, bytes)?),
        _ => None,
    };

    let pool = CubePool::from_json_texts(
        &utf8(cube_path, &cube_bytes)?,
        &utf8(gaps_path, &gaps_bytes)?,
        fence_text.as_deref(),
    )
    .map_err(|error| error.to_string())?;

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
        cube_list: Some(sha256_hex(&cube_bytes)),
        fence_record: fence_bytes.as_deref().map(sha256_hex),
        card_index: None,
        cube_table: None,
    };

    let mut caps = PodCaps::training_a2();
    caps.watchdog_seconds = watchdog_seconds;

    let pod_end = pod_start
        .checked_add(pods)
        .ok_or("--pod-start + --pods overflows u64")?;
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

    // Publish nothing unless the entire batch completes without a stopped pod
    // or watchdog expiry.
    let records = run_uniform_training_batch(&pool, &batch_config, &UniformPilot)
        .map_err(|error| error.to_string())?;
    let report = BatchReport::from_records(&records);

    match out_path {
        Some(path) => {
            let file = File::create_new(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
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
            writer
                .flush()
                .map_err(|error| format!("stdout: {error}"))?;
        }
    }

    let report_json =
        serde_json::to_string_pretty(&report).expect("a batch report is plain data");
    if let Some(path) = report_path {
        let mut file =
            File::create_new(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        writeln!(file, "{report_json}")
            .map_err(|error| format!("{}: {error}", path.display()))?;
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
