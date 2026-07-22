//! Fail-closed typed A-B-B-A launcher for the cell-zero evidence campaign.
//!
//! Drives four fresh processes in the fixed ordinal schedule
//! stock, unit, unit, stock against two pre-built probe executables,
//! computes each executable's SHA-256 itself, truncates each run's kernel
//! log before launch, requires zero exit status, parses each run's
//! CELL_ZERO_EVIDENCE_JSONL record, and fail-closed verifies the campaign
//! invariants: identical CPU-reference bits, fixture SHA, snapshot SHA, and
//! criteria SHA across all four runs; bit-identical device outputs within
//! each arm; distinct tracked-tree SHAs between arms; arm strings matching
//! the schedule; and clean builds everywhere. Emits one typed campaign
//! record naming ordinals, process ids, exit statuses, per-run record
//! digests, and every invariant verdict. The campaign record is
//! characterization until the shared metric core lands; it says so itself.
//!
//! Usage:
//!   cell_zero_abba_launcher_v1 <stock_exe> <unit_exe> <output_dir>

use sha2::{Digest, Sha256};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

const SCHEDULE_V1: [(usize, &str); 4] = [
    (0, "stock-cmma"),
    (1, "simple-unit-fork"),
    (2, "simple-unit-fork"),
    (3, "stock-cmma"),
];
const PROBE_FILTER_V1: &str = "cell_zero_raw_forward_probe_v1";
const RECORD_PREFIX_V1: &str = "CELL_ZERO_EVIDENCE_JSONL ";

fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sha256_hex_file(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(sha256_hex_bytes(&std::fs::read(path)?))
}

#[derive(serde::Serialize)]
struct LauncherRunRecordV1 {
    ordinal: usize,
    arm: &'static str,
    process_id: u32,
    exit_code: i32,
    executable_sha256: String,
    record_sha256: String,
    record_path: String,
    kernel_log_path: String,
}

#[derive(serde::Serialize)]
struct LauncherInvariantV1 {
    name: &'static str,
    passed: bool,
    detail: String,
}

#[derive(serde::Serialize)]
struct LauncherCampaignRecordV1 {
    schema: &'static str,
    evidence_status: &'static str,
    training_restart_authorized: bool,
    schedule: &'static str,
    runs: Vec<LauncherRunRecordV1>,
    invariants: Vec<LauncherInvariantV1>,
    all_invariants_passed: bool,
}

fn field<'a>(record: &'a serde_json::Value, name: &str) -> Result<&'a str, Box<dyn Error>> {
    record
        .get(name)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("record missing string field {name}").into())
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() != 4 {
        return Err("usage: cell_zero_abba_launcher_v1 <stock_exe> <unit_exe> <output_dir>".into());
    }
    let stock_exe = PathBuf::from(&arguments[1]);
    let unit_exe = PathBuf::from(&arguments[2]);
    let output_dir = PathBuf::from(&arguments[3]);
    std::fs::create_dir_all(&output_dir)?;

    let stock_sha = sha256_hex_file(&stock_exe)?;
    let unit_sha = sha256_hex_file(&unit_exe)?;
    if stock_sha == unit_sha {
        return Err("stock and unit executables are byte-identical; arms are not distinct".into());
    }

    let mut runs = Vec::new();
    let mut parsed_records = Vec::new();
    for (ordinal, arm) in SCHEDULE_V1 {
        let (exe, exe_sha) = if arm == "stock-cmma" {
            (&stock_exe, &stock_sha)
        } else {
            (&unit_exe, &unit_sha)
        };
        let kernel_log = output_dir.join(format!("kernel-log-{ordinal}-{arm}.log"));
        // Truncate so a stale log can never satisfy the route-evidence digest.
        std::fs::write(&kernel_log, b"")?;
        let record_path = output_dir.join(format!("record-{ordinal}-{arm}.jsonl"));

        let mut child = Command::new(exe)
            .args([
                PROBE_FILTER_V1,
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("CELL_ZERO_CONFIG", arm)
            .env("CELL_ZERO_EXE_SHA256", exe_sha)
            .env("CUBECL_DEBUG_LOG", &kernel_log)
            .env("CELL_ZERO_KERNEL_LOG", &kernel_log)
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        let process_id = child.id();
        let output = child.wait_with_output()?;
        let exit_code = output.status.code().ok_or("probe terminated by signal")?;
        if exit_code != 0 {
            return Err(format!("ordinal {ordinal} ({arm}) exited {exit_code}").into());
        }
        let stdout = String::from_utf8(output.stdout)?;
        let record_line = stdout
            .lines()
            .find_map(|line| {
                line.find(RECORD_PREFIX_V1)
                    .map(|index| &line[index + RECORD_PREFIX_V1.len()..])
            })
            .ok_or_else(|| format!("ordinal {ordinal} emitted no evidence record"))?;
        let parsed: serde_json::Value = serde_json::from_str(record_line)?;
        if field(&parsed, "arm")? != arm {
            return Err(format!("ordinal {ordinal} record arm mismatch").into());
        }
        std::fs::write(&record_path, record_line)?;
        runs.push(LauncherRunRecordV1 {
            ordinal,
            arm,
            process_id,
            exit_code,
            executable_sha256: exe_sha.clone(),
            record_sha256: sha256_hex_bytes(record_line.as_bytes()),
            record_path: record_path.display().to_string(),
            kernel_log_path: kernel_log.display().to_string(),
        });
        parsed_records.push(parsed);
    }

    let mut invariants = Vec::new();
    let mut check = |name: &'static str, passed: bool, detail: String| {
        invariants.push(LauncherInvariantV1 {
            name,
            passed,
            detail,
        });
    };

    let equal_across_all = |field_name: &str| -> Result<(bool, String), Box<dyn Error>> {
        let first = field(&parsed_records[0], field_name)?.to_owned();
        let all_equal = parsed_records
            .iter()
            .skip(1)
            .try_fold(true, |accumulator, record| {
                Ok::<bool, Box<dyn Error>>(accumulator && field(record, field_name)? == first)
            })?;
        Ok((all_equal, first))
    };

    let (cpu_equal, cpu_detail) = equal_across_all("cpu_bits_sha256")?;
    check(
        "cpu_reference_bits_identical_across_all",
        cpu_equal,
        cpu_detail,
    );
    let (fixture_equal, fixture_detail) = equal_across_all("fixture_sha256")?;
    check(
        "fixture_identical_across_all",
        fixture_equal,
        fixture_detail,
    );
    let (snapshot_equal, snapshot_detail) = equal_across_all("snapshot_state_sha256")?;
    check(
        "snapshot_identical_across_all",
        snapshot_equal,
        snapshot_detail,
    );
    let (criteria_equal, criteria_detail) = equal_across_all("criteria_sha256")?;
    check(
        "criteria_identical_across_all",
        criteria_equal,
        criteria_detail,
    );

    let device_bits: Vec<&str> = parsed_records
        .iter()
        .map(|record| field(record, "device_bits_sha256"))
        .collect::<Result<_, _>>()?;
    check(
        "stock_arm_device_bits_identical",
        device_bits[0] == device_bits[3],
        format!("{} vs {}", device_bits[0], device_bits[3]),
    );
    check(
        "unit_arm_device_bits_identical",
        device_bits[1] == device_bits[2],
        format!("{} vs {}", device_bits[1], device_bits[2]),
    );
    check(
        "arms_device_bits_distinct",
        device_bits[0] != device_bits[1],
        format!("{} vs {}", device_bits[0], device_bits[1]),
    );

    let trees: Vec<&str> = parsed_records
        .iter()
        .map(|record| field(record, "build_tracked_tree_sha256"))
        .collect::<Result<_, _>>()?;
    check(
        "arm_tracked_trees_distinct",
        trees[0] != trees[1] && trees[0] == trees[3] && trees[1] == trees[2],
        format!("stock {} unit {}", trees[0], trees[1]),
    );
    let clean = parsed_records
        .iter()
        .map(|record| field(record, "build_git_clean"))
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .all(|value| *value == "true");
    check("all_builds_clean", clean, String::new());

    let all_invariants_passed = invariants.iter().all(|invariant| invariant.passed);
    let campaign = LauncherCampaignRecordV1 {
        schema: "mtg-kernel-cell-zero-abba-campaign/v1",
        evidence_status: "characterization-pending-shared-core",
        training_restart_authorized: false,
        schedule: "stock-cmma, simple-unit-fork, simple-unit-fork, stock-cmma",
        runs,
        invariants,
        all_invariants_passed,
    };
    let campaign_line = serde_json::to_string(&campaign)?;
    std::fs::write(output_dir.join("campaign.jsonl"), &campaign_line)?;
    println!("CELL_ZERO_CAMPAIGN_JSONL {campaign_line}");
    if !all_invariants_passed {
        return Err("campaign invariants failed".into());
    }
    Ok(())
}
