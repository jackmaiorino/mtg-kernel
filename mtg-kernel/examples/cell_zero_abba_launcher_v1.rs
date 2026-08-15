//! Fail-closed typed A-B-B-A launcher for the cell-zero evidence campaign,
//! second revision closing the launcher-side audit findings.
//!
//! Schedule: stock, unit, unit, stock as four fresh processes. Hardening in
//! this revision: the output root must not pre-exist (a failed rerun can
//! never leave a stale green campaign visible); each executable is COPIED
//! into the output root, hashed, and the copy is what executes, closing the
//! hash-then-execute TOCTOU; each per-run record is deserialized into a
//! strict schema-exact typed struct rejecting unknown fields, and exactly
//! one record line is required per run; the sealed criteria digest and
//! non-authorizing state are enforced against the launcher's own embedded
//! copy of the criteria artifact; and arm-to-build-identity consistency is
//! checked (each arm's runs share one compiled head and tracked tree, the
//! two arms' pairs are distinct), which together with the probes' own
//! compile-bound arm constants makes an executable-argument swap fail
//! closed in both layers.
//!
//! Usage:
//!   cell_zero_abba_launcher_v1 <stock_exe> <unit_exe> <fresh_output_dir>

use mtg_kernel::native_cuda_qualification_metrics_v1 as metrics;
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
const RECORD_SCHEMA_V1: &str = "mtg-kernel-cell-zero-raw-forward-evidence/v3";
const CRITERIA_SHA256_V1: &str = "4c3d3249003bc34cae59166fc6dbcf1a71f0fd1d34758372564af394284aba28";
const CRITERIA_BYTES_V1: &[u8] =
    include_bytes!("../data/native_cuda_qualification_v1/criteria_v1.json");

fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Strict schema mirror of the probe's evidence record. Unknown fields are
/// rejected so schema drift between probe and launcher fails loudly.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct EvidenceCaseRawV1 {
    case: usize,
    actions: usize,
    device_logit_bits: Vec<u32>,
    cpu_logit_bits: Vec<u32>,
    device_value_bits: u32,
    cpu_value_bits: u32,
    max_abs: f64,
    range: f64,
    min_delta_index: usize,
    max_delta_index: usize,
    value_abs: f64,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct EvidenceWorstValueV1 {
    case: usize,
    device_value_bits: u32,
    cpu_value_bits: u32,
    abs_error: f64,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct EvidenceRecordV1 {
    schema: String,
    evidence_status: String,
    training_restart_authorized: bool,
    criteria_sha256: String,
    arm: String,
    build_git_head: String,
    build_git_clean: String,
    build_tracked_tree_sha256: String,
    build_tracked_tree_contract: String,
    executable_sha256: String,
    fork_upstream_burn_cubecl_sha: String,
    snapshot_state_sha256: String,
    fixture_sha256: String,
    case_count: usize,
    case_order: String,
    device_runtime_manifest_sha256: String,
    numerical_mode_claim: String,
    device_manifest_numerical_mode_note: String,
    kernel_log_sha256: String,
    cpu_bits_sha256: String,
    device_bits_sha256: String,
    global_max_abs: f64,
    global_max_range: f64,
    global_max_value_abs: f64,
    worst_value: EvidenceWorstValueV1,
    per_case: Vec<EvidenceCaseRawV1>,
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
    criteria_sha256: &'static str,
    schedule: &'static str,
    runs: Vec<LauncherRunRecordV1>,
    invariants: Vec<LauncherInvariantV1>,
    all_invariants_passed: bool,
}

fn copy_and_hash(
    exe: &Path,
    output_dir: &Path,
    label: &str,
) -> Result<(PathBuf, String), Box<dyn Error>> {
    let bytes = std::fs::read(exe)?;
    let digest = sha256_hex_bytes(&bytes);
    let copy_path = output_dir.join(format!("probe-{label}.exe"));
    std::fs::write(&copy_path, &bytes)?;
    // The COPY is what executes: the hash and the executed bytes are the
    // same read, closing the hash-then-execute TOCTOU on the source path.
    Ok((copy_path, digest))
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() != 4 {
        return Err(
            "usage: cell_zero_abba_launcher_v1 <stock_exe> <unit_exe> <fresh_output_dir>".into(),
        );
    }
    let output_dir = PathBuf::from(&arguments[3]);
    if output_dir.exists() {
        return Err("output dir must not pre-exist: a rerun may never reuse a stale root".into());
    }
    std::fs::create_dir_all(&output_dir)?;

    assert_eq!(
        sha256_hex_bytes(CRITERIA_BYTES_V1),
        CRITERIA_SHA256_V1,
        "launcher's embedded criteria bytes do not match the sealed digest"
    );

    let (stock_exe, stock_sha) = copy_and_hash(Path::new(&arguments[1]), &output_dir, "stock")?;
    let (unit_exe, unit_sha) = copy_and_hash(Path::new(&arguments[2]), &output_dir, "unit")?;
    if stock_sha == unit_sha {
        return Err("stock and unit executables are byte-identical; arms are not distinct".into());
    }

    let mut runs = Vec::new();
    let mut records = Vec::new();
    for (ordinal, arm) in SCHEDULE_V1 {
        let (exe, exe_sha) = if arm == "stock-cmma" {
            (&stock_exe, &stock_sha)
        } else {
            (&unit_exe, &unit_sha)
        };
        let kernel_log = output_dir.join(format!("kernel-log-{ordinal}-{arm}.log"));
        std::fs::write(&kernel_log, b"")?;
        let record_path = output_dir.join(format!("record-{ordinal}-{arm}.jsonl"));

        let child = Command::new(exe)
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
        let record_lines: Vec<&str> = stdout
            .lines()
            .filter_map(|line| {
                line.find(RECORD_PREFIX_V1)
                    .map(|index| &line[index + RECORD_PREFIX_V1.len()..])
            })
            .collect();
        if record_lines.len() != 1 {
            return Err(format!(
                "ordinal {ordinal} emitted {} evidence records; exactly one is required",
                record_lines.len()
            )
            .into());
        }
        let record_line = record_lines[0];
        let record: EvidenceRecordV1 = serde_json::from_str(record_line)?;
        if record.schema != RECORD_SCHEMA_V1 {
            return Err(format!("ordinal {ordinal} record schema {}", record.schema).into());
        }
        if record.arm != arm {
            return Err(format!("ordinal {ordinal} record arm mismatch: {}", record.arm).into());
        }
        if record.training_restart_authorized {
            return Err("record claims training authorization; forbidden".into());
        }
        if record.criteria_sha256 != CRITERIA_SHA256_V1 {
            return Err(format!("ordinal {ordinal} criteria digest mismatch").into());
        }
        if record.executable_sha256 != *exe_sha {
            return Err(format!("ordinal {ordinal} executable digest mismatch").into());
        }
        if record.build_git_clean != "true" {
            return Err(format!("ordinal {ordinal} built from an unclean tree").into());
        }
        for case in &record.per_case {
            if case.device_logit_bits.len() != case.actions
                || case.cpu_logit_bits.len() != case.actions
            {
                return Err(format!(
                    "ordinal {ordinal} case {} raw array shape mismatch",
                    case.case
                )
                .into());
            }
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
        records.push(record);
    }

    // Independent shared-metric-core recompute over the framed raw arrays:
    // every recomputable channel is rebuilt from bits and bit-compared to
    // the in-record aggregates, and the sealed caps are evaluated per case.
    let row_cap = metrics::cap_from_bits_v1("3f5d7dbf487fcb92");
    let value_cap = metrics::cap_from_bits_v1("3f60624dd2f1a9fb");
    let kl_cap = metrics::cap_from_bits_v1("3ee9f4631cd6b312");
    let mut recompute_all_match = true;
    let mut recompute_detail = String::new();
    let mut cap_summary = Vec::new();
    for (run_ordinal, record) in records.iter().enumerate() {
        let mut worst_kl = 0.0_f64;
        let mut rows_over_row_cap = 0_u64;
        let mut values_over_cap = 0_u64;
        let mut global_max_range = 0.0_f64;
        let mut global_max_abs = 0.0_f64;
        let mut global_max_value = 0.0_f64;
        for case in &record.per_case {
            let stats = metrics::row_delta_stats_v1(&case.device_logit_bits, &case.cpu_logit_bits);
            if stats.range.to_bits() != case.range.to_bits()
                || stats.max_abs.to_bits() != case.max_abs.to_bits()
                || stats.min_delta_index != case.min_delta_index
                || stats.max_delta_index != case.max_delta_index
            {
                recompute_all_match = false;
                recompute_detail =
                    format!("run {run_ordinal} case {} delta stats mismatch", case.case);
            }
            let value_abs = (f64::from(f32::from_bits(case.device_value_bits))
                - f64::from(f32::from_bits(case.cpu_value_bits)))
            .abs();
            if value_abs.to_bits() != case.value_abs.to_bits() {
                recompute_all_match = false;
                recompute_detail = format!("run {run_ordinal} case {} value mismatch", case.case);
            }
            let kl_forward = metrics::row_kl_v1(&case.cpu_logit_bits, &case.device_logit_bits);
            let kl_backward = metrics::row_kl_v1(&case.device_logit_bits, &case.cpu_logit_bits);
            worst_kl = worst_kl.max(kl_forward).max(kl_backward);
            if !metrics::within_cap_v1(stats.range, row_cap) {
                rows_over_row_cap += 1;
            }
            if !metrics::within_cap_v1(value_abs, value_cap) {
                values_over_cap += 1;
            }
            global_max_range = global_max_range.max(stats.range);
            global_max_abs = global_max_abs.max(stats.max_abs);
            global_max_value = global_max_value.max(value_abs);
        }
        if global_max_range.to_bits() != record.global_max_range.to_bits()
            || global_max_abs.to_bits() != record.global_max_abs.to_bits()
            || global_max_value.to_bits() != record.global_max_value_abs.to_bits()
        {
            recompute_all_match = false;
            recompute_detail = format!("run {run_ordinal} global aggregate mismatch");
        }
        cap_summary.push(format!(
            "run {run_ordinal} ({}): rows_over_row_cap={rows_over_row_cap}              values_over_cap={values_over_cap} worst_row_kl={worst_kl:e}              kl_within_cap={}",
            record.arm,
            metrics::within_cap_v1(worst_kl, kl_cap)
        ));
    }

    let mut invariants = Vec::new();
    let mut check = |name: &'static str, passed: bool, detail: String| {
        invariants.push(LauncherInvariantV1 {
            name,
            passed,
            detail,
        });
    };

    let equal_across_all = |accessor: fn(&EvidenceRecordV1) -> &str| {
        let first = accessor(&records[0]).to_owned();
        let all = records.iter().all(|record| accessor(record) == first);
        (all, first)
    };

    let (cpu_equal, cpu_detail) = equal_across_all(|record| &record.cpu_bits_sha256);
    check(
        "cpu_reference_bits_identical_across_all",
        cpu_equal,
        cpu_detail,
    );
    let (fixture_equal, fixture_detail) = equal_across_all(|record| &record.fixture_sha256);
    check(
        "fixture_identical_across_all",
        fixture_equal,
        fixture_detail,
    );
    let (snapshot_equal, snapshot_detail) =
        equal_across_all(|record| &record.snapshot_state_sha256);
    check(
        "snapshot_identical_across_all",
        snapshot_equal,
        snapshot_detail,
    );
    let (criteria_equal, criteria_detail) = equal_across_all(|record| &record.criteria_sha256);
    check(
        "criteria_identical_across_all",
        criteria_equal,
        criteria_detail,
    );

    check(
        "stock_arm_device_bits_identical",
        records[0].device_bits_sha256 == records[3].device_bits_sha256,
        format!(
            "{} vs {}",
            records[0].device_bits_sha256, records[3].device_bits_sha256
        ),
    );
    check(
        "unit_arm_device_bits_identical",
        records[1].device_bits_sha256 == records[2].device_bits_sha256,
        format!(
            "{} vs {}",
            records[1].device_bits_sha256, records[2].device_bits_sha256
        ),
    );
    check(
        "arms_device_bits_distinct",
        records[0].device_bits_sha256 != records[1].device_bits_sha256,
        String::new(),
    );

    // Arm-to-build-identity consistency: within an arm one (head, tree)
    // pair; between arms distinct pairs. Combined with the probes' own
    // compile-bound arm constants this closes the argument-swap relabeling.
    let stock_pair = (
        records[0].build_git_head.clone(),
        records[0].build_tracked_tree_sha256.clone(),
    );
    let unit_pair = (
        records[1].build_git_head.clone(),
        records[1].build_tracked_tree_sha256.clone(),
    );
    check(
        "arm_build_identity_consistent",
        records[3].build_git_head == stock_pair.0
            && records[3].build_tracked_tree_sha256 == stock_pair.1
            && records[2].build_git_head == unit_pair.0
            && records[2].build_tracked_tree_sha256 == unit_pair.1
            && stock_pair != unit_pair,
        format!(
            "stock ({}, {}) unit ({}, {})",
            &stock_pair.0[..8.min(stock_pair.0.len())],
            &stock_pair.1[..8.min(stock_pair.1.len())],
            &unit_pair.0[..8.min(unit_pair.0.len())],
            &unit_pair.1[..8.min(unit_pair.1.len())]
        ),
    );
    check(
        "numerical_mode_claims_distinct_per_arm",
        records[0].numerical_mode_claim == records[3].numerical_mode_claim
            && records[1].numerical_mode_claim == records[2].numerical_mode_claim
            && records[0].numerical_mode_claim != records[1].numerical_mode_claim,
        format!(
            "stock '{}' unit '{}'",
            records[0].numerical_mode_claim, records[1].numerical_mode_claim
        ),
    );

    check(
        "shared_metric_core_recompute_matches_records",
        recompute_all_match,
        recompute_detail,
    );
    check(
        "cap_verdicts_characterization",
        true,
        cap_summary.join("; "),
    );

    let all_invariants_passed = invariants.iter().all(|invariant| invariant.passed);
    let campaign = LauncherCampaignRecordV1 {
        schema: "mtg-kernel-cell-zero-abba-campaign/v2",
        evidence_status: "characterization-pending-shared-core",
        training_restart_authorized: false,
        criteria_sha256: CRITERIA_SHA256_V1,
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
