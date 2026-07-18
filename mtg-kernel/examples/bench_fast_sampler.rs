//! Audited, pinned-thread benchmark for the complete fast-sampler decision path.
//!
//! Canonical runs require a runtime evidence manifest. Diagnostic smoke runs use
//! noncanonical repeat/duration arguments and never emit canonical evidence.

use mtg_kernel::fast_sampler::FastCategoricalScratch;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::hint::{black_box, spin_loop};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const TARGET_DECISIONS_PER_SECOND: f64 = 1_250_000.0;
const CANONICAL_REPEATS: usize = 5;
const CANONICAL_DURATION_MS: u64 = 2_000;
const MAXIMUM_START_DELAY_NS: u64 = 25_000_000;
const MAXIMUM_OVERSHOOT_NS: u64 = 25_000_000;
const MAXIMUM_CPU_ALIGNMENT_SLACK_NS: u64 = 5_000_000;
const MAXIMUM_EXTERNAL_CPU_BUSY_FRACTION: f64 = 0.10;
const CHUNK_DECISIONS: usize = 128;
const MANIFEST_SCHEMA: &str = "fast_sampler_benchmark_evidence_manifest/v2";
const EVIDENCE_SCHEMA: &str = "fast_sampler_benchmark_validation_evidence/v1";
const VALIDATION_RECORD_SCHEMA: &str = "fast_sampler_benchmark_validation_record/v2";
const EVIDENCE_PREFIX: &str = "evidence/fast_sampler/";
const EVIDENCE_DIRECTORY: &str = "evidence/fast_sampler";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const CARGO_BUILD_COMMAND_IDENTITY: &str =
    "cargo build --release --locked -p mtg-kernel --example bench_fast_sampler";
const REJECTED_BUILD_OVERRIDE_CONTRACT: &str = "no RUSTFLAGS, CARGO_ENCODED_RUSTFLAGS, CARGO_BUILD_RUSTFLAGS, RUSTC, RUSTC_WRAPPER, RUSTC_WORKSPACE_WRAPPER, CARGO_BUILD_TARGET, CARGO_INCREMENTAL, CARGO_PROFILE_RELEASE_*, CARGO_TARGET_*_RUSTFLAGS, or CARGO_TARGET_*_LINKER environment override";
const OBSERVED_PROFILE_STATUS: &str = "observed_provenance_bound";
const OBSERVED_PROFILE_SCOPE: &str =
    "all_sampled_policy_decisions_in_rally_vs_rally_not_learner_only";
const PROVISIONAL_PROFILE_STATUS: &str = "provisional_synthetic";
const PROVISIONAL_PROFILE_SCOPE: &str = "provisional_synthetic_not_observed_policy_decisions";
const RALLY_SOURCE_HEAD: &str = "d71dca82dfe36292328ecbc4962a0d6764d9ca5c";
const RALLY_SOURCE_MANIFEST_SHA256: &str =
    "09e816949de05d76cf37148e015eb973b4f6568e256e755e5b727480df56d9d3";
const RALLY_ENVIRONMENT_BINARY_SHA256: &str =
    "b81b5ad88e6f728922b8635405aead28588066b2563cdd9644439100715d4c51";
const RALLY_BENCHMARK_BINARY_SHA256: &str =
    "04802ed2cb953b6ef0f071f42304221de16fd9f411b8decc025ffbfa56b1fbe8";
static PUBLICATION_NONCE: AtomicU64 = AtomicU64::new(0);

const FAST_SAMPLER_SOURCE: &[u8] = include_bytes!("../src/fast_sampler.rs");
const LIB_SOURCE: &[u8] = include_bytes!("../src/lib.rs");
const BENCHMARK_SOURCE: &[u8] = include_bytes!("bench_fast_sampler.rs");
const CRATE_MANIFEST: &[u8] = include_bytes!("../Cargo.toml");
const WORKSPACE_MANIFEST: &[u8] = include_bytes!("../../Cargo.toml");
const CARGO_LOCK: &[u8] = include_bytes!("../../Cargo.lock");
const TOOLCHAIN: &[u8] = include_bytes!("../../rust-toolchain.toml");
const ORACLE: &[u8] = include_bytes!("../../data/fast_sampler_decimal_oracle_v1.json");

const SOURCE_FILES: [(&str, &[u8]); 8] = [
    ("mtg-kernel/src/fast_sampler.rs", FAST_SAMPLER_SOURCE),
    ("mtg-kernel/src/lib.rs", LIB_SOURCE),
    (
        "mtg-kernel/examples/bench_fast_sampler.rs",
        BENCHMARK_SOURCE,
    ),
    ("mtg-kernel/Cargo.toml", CRATE_MANIFEST),
    ("Cargo.toml", WORKSPACE_MANIFEST),
    ("Cargo.lock", CARGO_LOCK),
    ("rust-toolchain.toml", TOOLCHAIN),
    ("data/fast_sampler_decimal_oracle_v1.json", ORACLE),
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EvidenceManifest {
    schema_version: String,
    expected_clean_revision: String,
    expected_source_bundle_sha256: String,
    expected_cargo_lock_sha256: String,
    expected_toolchain_sha256: String,
    expected_executable_sha256: String,
    expected_rustc_verbose_sha256: String,
    expected_cargo_build_command_identity: String,
    expected_rejected_build_override_contract_sha256: String,
    expected_build_override_state_sha256: String,
    evidence_output_path: String,
}

#[derive(Clone, Debug)]
struct LoadedManifest {
    repo_relative_path: String,
    bytes_sha256: String,
    manifest: EvidenceManifest,
}

#[derive(Deserialize)]
struct OracleFile {
    schema_version: u32,
    workload_width_profile: WorkloadWidthProfile,
    cases: Vec<OracleCase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WorkloadWidthProfile {
    status: String,
    claim_eligible: bool,
    scope: String,
    source_artifact: Option<String>,
    source_artifact_sha256: Option<String>,
    source_schema: Option<String>,
    source_aggregate_record_sha256: Option<String>,
    source_provenance_class: Option<String>,
    source_performance_gate_valid: Option<bool>,
    source_performance_rates_included: Option<bool>,
    source_coverage_scope: Option<String>,
    raw_source_artifact_sha256: Option<String>,
    raw_source_artifact_size_bytes: Option<u64>,
    source_head_before: Option<String>,
    source_head_after: Option<String>,
    source_worktree_state_before: Option<String>,
    source_worktree_state_after: Option<String>,
    source_status_sha256_before: Option<String>,
    source_status_sha256_after: Option<String>,
    source_manifest_file_count: Option<u64>,
    source_manifest_sha256_before: Option<String>,
    source_manifest_sha256_after: Option<String>,
    bound_build_source_manifest_sha256_before: Option<String>,
    bound_build_source_manifest_sha256_after: Option<String>,
    source_attestations_stable: Option<bool>,
    environment_binary_sha256_prebuild: Option<String>,
    environment_binary_sha256_postbuild: Option<String>,
    environment_binary_sha256_before: Option<String>,
    environment_binary_sha256_after: Option<String>,
    benchmark_binary_sha256_prebuild: Option<String>,
    benchmark_binary_sha256_postbuild: Option<String>,
    benchmark_binary_sha256_before: Option<String>,
    benchmark_binary_sha256_after: Option<String>,
    binary_attestations_stable: Option<bool>,
    formal_binary_source_attestation_present: Option<bool>,
    compiled_input_closure_attested: Option<bool>,
    statistics: WidthStatistics,
    histogram: Vec<WidthHistogramEntry>,
    final_all_nine_deck_gate: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WidthStatistics {
    sample_count: u64,
    mean: f64,
    nearest_rank_p95: usize,
    maximum: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WidthHistogramEntry {
    width: usize,
    policy_decision_count: u64,
}

#[derive(Deserialize)]
struct OracleCase {
    classification: String,
    width_profile_count: u64,
    logit_bits_hex: Vec<String>,
}

#[derive(Clone, Debug)]
struct WeightedCase {
    upper_bound: u64,
    logits: Vec<f32>,
}

#[derive(Clone, Debug)]
struct Workload {
    profile: WorkloadWidthProfile,
    cases: Vec<WeightedCase>,
    total_weight: u64,
}

impl Workload {
    fn logits_for(&self, ordinal: u64) -> &[f32] {
        let target = ordinal % self.total_weight;
        self.cases
            .iter()
            .find(|case| target < case.upper_bound)
            .map(|case| case.logits.as_slice())
            .expect("validated workload cumulative bounds must be total")
    }
}

#[derive(Clone, Copy)]
struct Schedule {
    start: Instant,
    deadline: Instant,
}

#[derive(Clone, Debug, Serialize)]
struct WorkerResult {
    worker: usize,
    target_cpu: usize,
    observed_cpu: Option<u32>,
    affinity_valid: bool,
    start_delay_ns: u64,
    finish_overshoot_ns: u64,
    finish_offset_ns: u64,
    decisions: u64,
    selection_checksum: u64,
    error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct CpuAccounting {
    probe_valid: bool,
    arithmetic_valid: bool,
    alignment_valid: bool,
    start_alignment_slack_ns: u64,
    end_alignment_slack_ns: u64,
    system_total_capacity_ticks: Option<u64>,
    system_busy_ticks: Option<u64>,
    benchmark_process_ticks: Option<u64>,
    external_busy_ticks: Option<u64>,
    external_busy_fraction_of_total_capacity: Option<f64>,
    external_busy_fraction_within_bound: bool,
    error_code: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct RepeatResult {
    repeat: usize,
    threads: usize,
    requested_duration_ms: u64,
    common_wall_clock_ns: u64,
    aggregate_decisions: u64,
    decisions_per_second: f64,
    maximum_start_delay_ns: u64,
    maximum_finish_overshoot_ns: u64,
    minimum_worker_decisions: u64,
    maximum_worker_decisions: u64,
    affinity_valid: bool,
    timing_valid: bool,
    cpu_accounting: CpuAccounting,
    workers: Vec<WorkerResult>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct RuntimeAttestation {
    binary_sha256: String,
    embedded_source_bundle_sha256: String,
    disk_source_bundle_sha256: String,
    cargo_lock_sha256: String,
    toolchain_sha256: String,
    git_head: String,
    tracked_git_status_sha256: String,
    tracked_git_status_line_count: usize,
    tracked_git_diff_sha256: String,
    rustc_verbose_sha256: String,
    rustc_release: String,
    rustc_host: String,
    cargo_build_command_identity: String,
    rejected_build_override_contract_sha256: String,
    build_override_state_sha256: String,
    build_override_present_count: usize,
    processor_identifier: String,
    available_parallelism: usize,
    number_of_processors: String,
    os_version_sha256: String,
    active_power_scheme_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuildOverrideSnapshot {
    state_sha256: String,
    present_count: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
struct BlockedProcessCategoryCounts {
    rust_build_toolchain: usize,
    python_runtime: usize,
    benchmark_like: usize,
}

impl BlockedProcessCategoryCounts {
    fn total(&self) -> usize {
        self.rust_build_toolchain + self.python_runtime + self.benchmark_like
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ProcessProbe {
    valid: bool,
    observed_process_count: usize,
    blocked_category_counts: BlockedProcessCategoryCounts,
    blocked_process_count: usize,
    normalized_inventory_sha256: String,
    normalized_blocked_inventory_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct GpuInventory {
    probe_status: &'static str,
    reported_compute_process_count: usize,
    classified_compute_or_benchmark_count: usize,
    unidentified_process_count: usize,
    normalized_inventory_sha256: String,
    gate_policy: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct InterferenceSnapshot {
    process: ProcessProbe,
    gpu_inventory: GpuInventory,
}

#[derive(Clone, Copy, Debug)]
struct CpuTimeSnapshot {
    system_idle_ticks: u64,
    system_kernel_ticks: u64,
    system_user_ticks: u64,
    process_kernel_ticks: u64,
    process_user_ticks: u64,
}

#[derive(Clone, Debug)]
struct Arguments {
    repeats: usize,
    duration_ms: u64,
    evidence_manifest: Option<String>,
    write_manifest_template: Option<String>,
    template_evidence_output: Option<String>,
    benchmark_parameter_was_explicit: bool,
}

fn parse_arguments() -> Result<Arguments, Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let mut parsed = Arguments {
        repeats: CANONICAL_REPEATS,
        duration_ms: CANONICAL_DURATION_MS,
        evidence_manifest: None,
        write_manifest_template: None,
        template_evidence_output: None,
        benchmark_parameter_was_explicit: false,
    };
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--repeats" => {
                parsed.benchmark_parameter_was_explicit = true;
                parsed.repeats = arguments
                    .next()
                    .ok_or("--repeats requires a value")?
                    .parse()?;
            }
            "--duration-ms" => {
                parsed.benchmark_parameter_was_explicit = true;
                parsed.duration_ms = arguments
                    .next()
                    .ok_or("--duration-ms requires a value")?
                    .parse()?;
            }
            "--evidence-manifest" => {
                if parsed.evidence_manifest.is_some() {
                    return Err("--evidence-manifest may be supplied only once".into());
                }
                parsed.evidence_manifest = Some(
                    arguments
                        .next()
                        .ok_or("--evidence-manifest requires a value")?,
                );
            }
            "--write-manifest-template" => {
                if parsed.write_manifest_template.is_some() {
                    return Err("--write-manifest-template may be supplied only once".into());
                }
                parsed.write_manifest_template = Some(
                    arguments
                        .next()
                        .ok_or("--write-manifest-template requires a value")?,
                );
            }
            "--template-evidence-output" => {
                if parsed.template_evidence_output.is_some() {
                    return Err("--template-evidence-output may be supplied only once".into());
                }
                parsed.template_evidence_output = Some(
                    arguments
                        .next()
                        .ok_or("--template-evidence-output requires a value")?,
                );
            }
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    if parsed.repeats == 0 || parsed.duration_ms == 0 {
        return Err("repeats and duration must both be positive".into());
    }
    let template_mode =
        parsed.write_manifest_template.is_some() || parsed.template_evidence_output.is_some();
    if template_mode {
        if parsed.write_manifest_template.is_none()
            || parsed.template_evidence_output.is_none()
            || parsed.evidence_manifest.is_some()
            || parsed.benchmark_parameter_was_explicit
        {
            return Err("manifest-template mode requires both template paths and forbids benchmark parameters or --evidence-manifest".into());
        }
        return Ok(parsed);
    }
    let canonical =
        parsed.repeats == CANONICAL_REPEATS && parsed.duration_ms == CANONICAL_DURATION_MS;
    if canonical != parsed.evidence_manifest.is_some() {
        return Err(
            "canonical parameters require --evidence-manifest; diagnostic parameters forbid it"
                .into(),
        );
    }
    Ok(parsed)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn bundle_sha256<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> String {
    let mut digest = Sha256::new();
    for (name, bytes) in files {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    format!("{:x}", digest.finalize())
}

fn validate_repo_relative_path(raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.contains('\\')
        || raw.contains(':')
        || raw.contains('\0')
        || raw
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("path must be a nonempty repo-relative slash path".to_owned());
    }
    if raw.split('/').any(|part| part.eq_ignore_ascii_case(".git")) {
        return Err("path must not address repository metadata".to_owned());
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Err("path must be repository relative".to_owned());
    }
    Ok(path)
}

fn validate_manifest_fields(manifest: &EvidenceManifest) -> Result<PathBuf, String> {
    if manifest.schema_version != MANIFEST_SCHEMA {
        return Err("unsupported evidence manifest schema".to_owned());
    }
    if !is_lower_hex(&manifest.expected_clean_revision, 40) {
        return Err("expected clean revision must be a lowercase 40-hex commit".to_owned());
    }
    for (label, digest) in [
        (
            "expected source bundle",
            &manifest.expected_source_bundle_sha256,
        ),
        ("expected Cargo.lock", &manifest.expected_cargo_lock_sha256),
        ("expected toolchain", &manifest.expected_toolchain_sha256),
        ("expected executable", &manifest.expected_executable_sha256),
        (
            "expected rustc -Vv output",
            &manifest.expected_rustc_verbose_sha256,
        ),
        (
            "expected rejected build-override contract",
            &manifest.expected_rejected_build_override_contract_sha256,
        ),
        (
            "expected build-override state",
            &manifest.expected_build_override_state_sha256,
        ),
    ] {
        if !is_lower_hex(digest, 64) {
            return Err(format!("{label} digest must be lowercase SHA-256"));
        }
    }
    if manifest.expected_cargo_build_command_identity != CARGO_BUILD_COMMAND_IDENTITY {
        return Err("evidence manifest build command identity drifted".to_owned());
    }
    if manifest.expected_rejected_build_override_contract_sha256
        != sha256_hex(REJECTED_BUILD_OVERRIDE_CONTRACT.as_bytes())
    {
        return Err("evidence manifest rejected build-override contract drifted".to_owned());
    }
    if manifest.expected_build_override_state_sha256 != EMPTY_SHA256 {
        return Err("evidence manifest must require an empty build-override state".to_owned());
    }
    if !manifest.evidence_output_path.starts_with(EVIDENCE_PREFIX)
        || !manifest.evidence_output_path.ends_with(".json")
    {
        return Err("evidence output must be a JSON file under evidence/fast_sampler/".to_owned());
    }
    validate_repo_relative_path(&manifest.evidence_output_path)
}

fn validate_evidence_child_json(raw: &str) -> Result<PathBuf, String> {
    let path = validate_repo_relative_path(raw)?;
    if path.parent() != Some(Path::new(EVIDENCE_DIRECTORY))
        || path.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(
            "template and evidence paths must be direct JSON children of evidence/fast_sampler/"
                .to_owned(),
        );
    }
    Ok(path)
}

fn command_output_at(
    repository_root: &Path,
    program: &str,
    arguments: &[&str],
) -> Result<String, Box<dyn Error>> {
    let output = Command::new(program)
        .current_dir(repository_root)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(format!("command failed: {program}").into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn command_stdout_bytes_at(
    repository_root: &Path,
    program: &str,
    arguments: &[&str],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let output = Command::new(program)
        .current_dir(repository_root)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(format!("command failed: {program}").into());
    }
    Ok(output.stdout)
}

fn repository_root() -> Result<PathBuf, Box<dyn Error>> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        return Err("git rev-parse --show-toplevel failed".into());
    }
    let reported = String::from_utf8(output.stdout)?;
    Ok(fs::canonicalize(reported.trim())?)
}

fn ensure_existing_path_within_root(
    repository_root: &Path,
    relative: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    let canonical = fs::canonicalize(repository_root.join(relative))?;
    if !canonical.starts_with(repository_root) {
        return Err("repo-relative path resolves outside repository".into());
    }
    Ok(canonical)
}

fn load_manifest(
    repository_root: &Path,
    repo_relative_path: &str,
) -> Result<LoadedManifest, Box<dyn Error>> {
    let relative = validate_repo_relative_path(repo_relative_path)?;
    let canonical = ensure_existing_path_within_root(repository_root, &relative)?;
    let bytes = fs::read(canonical)?;
    let manifest: EvidenceManifest = serde_json::from_slice(&bytes)?;
    let output_relative = validate_manifest_fields(&manifest)?;
    if relative == output_relative {
        return Err("evidence manifest and output paths must differ".into());
    }
    Ok(LoadedManifest {
        repo_relative_path: repo_relative_path.to_owned(),
        bytes_sha256: sha256_hex(&bytes),
        manifest,
    })
}

fn validate_output_target(
    repository_root: &Path,
    output_relative: &Path,
) -> Result<(), Box<dyn Error>> {
    let output = repository_root.join(output_relative);
    if output.exists() {
        return Err(
            "evidence output already exists; canonical evidence is never overwritten".into(),
        );
    }
    let parent = output.parent().ok_or("evidence output has no parent")?;
    let parent_relative = output_relative
        .parent()
        .ok_or("evidence output has no repo-relative parent")?;
    if !parent.is_dir() {
        return Err("evidence output parent directory must already exist".into());
    }
    validate_directory_chain_no_reparse(repository_root, parent_relative)?;
    let canonical_parent = fs::canonicalize(parent)?;
    if !canonical_parent.starts_with(repository_root) {
        return Err("evidence output parent resolves outside repository".into());
    }
    Ok(())
}

fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn validate_directory_chain_no_reparse(
    repository_root: &Path,
    relative: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut current = repository_root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err("evidence directory chain is not repo relative".into());
        };
        current.push(name);
        let metadata = fs::symlink_metadata(&current)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata_is_reparse(&metadata)
        {
            return Err(
                "evidence directory chain contains a non-directory or reparse point".into(),
            );
        }
        if !fs::canonicalize(&current)?.starts_with(repository_root) {
            return Err("evidence directory chain resolves outside repository".into());
        }
    }
    Ok(())
}

fn safe_prepare_evidence_directory(repository_root: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut current = repository_root.to_path_buf();
    for component in ["evidence", "fast_sampler"] {
        current.push(component);
        match fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let metadata = fs::symlink_metadata(&current)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata_is_reparse(&metadata)
        {
            return Err("evidence directory contains a non-directory or reparse component".into());
        }
        let canonical = fs::canonicalize(&current)?;
        if !canonical.starts_with(repository_root) {
            return Err("evidence directory resolves outside repository".into());
        }
    }
    Ok(current)
}

fn source_files_are_tracked(repository_root: &Path) -> Result<bool, Box<dyn Error>> {
    let mut command = Command::new("git");
    command
        .current_dir(repository_root)
        .args(["ls-files", "--error-unmatch", "--"]);
    for (name, _) in SOURCE_FILES {
        command.arg(name);
    }
    Ok(command.output()?.status.success())
}

fn manifest_from_attestation(
    attestation: &RuntimeAttestation,
    evidence_output_path: &str,
) -> EvidenceManifest {
    EvidenceManifest {
        schema_version: MANIFEST_SCHEMA.to_owned(),
        expected_clean_revision: attestation.git_head.clone(),
        expected_source_bundle_sha256: attestation.embedded_source_bundle_sha256.clone(),
        expected_cargo_lock_sha256: attestation.cargo_lock_sha256.clone(),
        expected_toolchain_sha256: attestation.toolchain_sha256.clone(),
        expected_executable_sha256: attestation.binary_sha256.clone(),
        expected_rustc_verbose_sha256: attestation.rustc_verbose_sha256.clone(),
        expected_cargo_build_command_identity: attestation.cargo_build_command_identity.clone(),
        expected_rejected_build_override_contract_sha256: attestation
            .rejected_build_override_contract_sha256
            .clone(),
        expected_build_override_state_sha256: attestation.build_override_state_sha256.clone(),
        evidence_output_path: evidence_output_path.to_owned(),
    }
}

fn write_manifest_template_workflow(
    repository_root: &Path,
    manifest_path: &str,
    evidence_output_path: &str,
) -> Result<(), Box<dyn Error>> {
    let manifest_relative = validate_evidence_child_json(manifest_path)?;
    let output_relative = validate_evidence_child_json(evidence_output_path)?;
    if manifest_relative == output_relative {
        return Err("manifest template and evidence output paths must differ".into());
    }

    let attestation = runtime_attestation(repository_root)?;
    let toolchain_pinned = attestation.rustc_release == "1.94.1"
        && String::from_utf8_lossy(TOOLCHAIN).contains("channel = \"1.94.1\"");
    if attestation.tracked_git_status_line_count != 0
        || attestation.tracked_git_status_sha256 != EMPTY_SHA256
        || attestation.tracked_git_diff_sha256 != EMPTY_SHA256
        || attestation.embedded_source_bundle_sha256 != attestation.disk_source_bundle_sha256
        || attestation.build_override_present_count != 0
        || attestation.build_override_state_sha256 != EMPTY_SHA256
        || cfg!(debug_assertions)
        || !toolchain_pinned
        || !source_files_are_tracked(repository_root)?
    {
        return Err("manifest template requires a release executable, clean committed source bundle, pinned compiler, and empty rejected build-override state".into());
    }

    safe_prepare_evidence_directory(repository_root)?;
    validate_output_target(repository_root, &manifest_relative)?;
    validate_output_target(repository_root, &output_relative)?;
    let manifest = manifest_from_attestation(&attestation, evidence_output_path);
    validate_manifest_fields(&manifest)?;
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    publish_bytes_atomic_no_replace(repository_root, &manifest_relative, &bytes)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema_version": "fast_sampler_benchmark_manifest_template_summary/v1",
            "status": "manifest_template_written",
            "manifest_path": manifest_path,
            "manifest_sha256": sha256_hex(&bytes),
            "evidence_output_path": evidence_output_path,
            "expected_executable_sha256": manifest.expected_executable_sha256,
            "expected_rustc_verbose_sha256": manifest.expected_rustc_verbose_sha256,
            "cargo_build_command_identity": manifest.expected_cargo_build_command_identity,
        }))?
    );
    Ok(())
}

fn rejected_build_override_name(name: &str) -> bool {
    matches!(
        name,
        "RUSTFLAGS"
            | "CARGO_ENCODED_RUSTFLAGS"
            | "CARGO_BUILD_RUSTFLAGS"
            | "RUSTC"
            | "RUSTC_WRAPPER"
            | "RUSTC_WORKSPACE_WRAPPER"
            | "CARGO_BUILD_TARGET"
            | "CARGO_INCREMENTAL"
    ) || name.starts_with("CARGO_PROFILE_RELEASE_")
        || (name.starts_with("CARGO_TARGET_")
            && (name.ends_with("_RUSTFLAGS") || name.ends_with("_LINKER")))
}

fn build_override_snapshot() -> BuildOverrideSnapshot {
    let mut entries = std::env::vars_os()
        .filter_map(|(name, value)| {
            let normalized_name = name.to_string_lossy().to_ascii_uppercase();
            rejected_build_override_name(&normalized_name)
                .then(|| (normalized_name, value.to_string_lossy().into_owned()))
        })
        .collect::<Vec<_>>();
    entries.sort_unstable();
    let mut digest = Sha256::new();
    for (name, value) in &entries {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value.as_bytes());
    }
    BuildOverrideSnapshot {
        state_sha256: format!("{:x}", digest.finalize()),
        present_count: entries.len(),
    }
}

fn runtime_attestation(repository_root: &Path) -> Result<RuntimeAttestation, Box<dyn Error>> {
    let binary = fs::read(std::env::current_exe()?)?;
    let embedded_source_bundle_sha256 = bundle_sha256(SOURCE_FILES);
    let mut disk_files = Vec::with_capacity(SOURCE_FILES.len());
    for (name, _) in SOURCE_FILES {
        disk_files.push((name, fs::read(repository_root.join(name))?));
    }
    let disk_source_bundle_sha256 = bundle_sha256(
        disk_files
            .iter()
            .map(|(name, bytes)| (*name, bytes.as_slice())),
    );
    let git_head = command_output_at(repository_root, "git", &["rev-parse", "HEAD"])?;
    let tracked_status = command_output_at(
        repository_root,
        "git",
        &["status", "--porcelain=v1", "--untracked-files=no"],
    )?;
    let tracked_diff =
        command_output_at(repository_root, "git", &["diff", "--binary", "HEAD", "--"])?;
    let rustc_verbose_bytes = command_stdout_bytes_at(repository_root, "rustc", &["-Vv"])?;
    let rustc_verbose = String::from_utf8(rustc_verbose_bytes.clone())?;
    let rustc_verbose_trimmed = rustc_verbose.trim();
    let rustc_release = rustc_verbose_trimmed
        .lines()
        .find_map(|line| line.strip_prefix("release: "))
        .unwrap_or("missing")
        .to_owned();
    let rustc_host = rustc_verbose_trimmed
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap_or("missing")
        .to_owned();
    let os_version = command_output_at(repository_root, "cmd", &["/c", "ver"])?;
    let power_scheme = command_output_at(repository_root, "powercfg", &["/getactivescheme"])?;
    let build_overrides = build_override_snapshot();

    Ok(RuntimeAttestation {
        binary_sha256: sha256_hex(&binary),
        embedded_source_bundle_sha256,
        disk_source_bundle_sha256,
        cargo_lock_sha256: sha256_hex(CARGO_LOCK),
        toolchain_sha256: sha256_hex(TOOLCHAIN),
        git_head,
        tracked_git_status_sha256: sha256_hex(tracked_status.as_bytes()),
        tracked_git_status_line_count: tracked_status.lines().count(),
        tracked_git_diff_sha256: sha256_hex(tracked_diff.as_bytes()),
        rustc_verbose_sha256: sha256_hex(&rustc_verbose_bytes),
        rustc_release,
        rustc_host,
        cargo_build_command_identity: CARGO_BUILD_COMMAND_IDENTITY.to_owned(),
        rejected_build_override_contract_sha256: sha256_hex(
            REJECTED_BUILD_OVERRIDE_CONTRACT.as_bytes(),
        ),
        build_override_state_sha256: build_overrides.state_sha256,
        build_override_present_count: build_overrides.present_count,
        processor_identifier: std::env::var("PROCESSOR_IDENTIFIER")
            .unwrap_or_else(|_| "unavailable".to_owned()),
        available_parallelism: thread::available_parallelism()?.get(),
        number_of_processors: std::env::var("NUMBER_OF_PROCESSORS")
            .unwrap_or_else(|_| "unavailable".to_owned()),
        os_version_sha256: sha256_hex(os_version.as_bytes()),
        active_power_scheme_sha256: sha256_hex(power_scheme.as_bytes()),
    })
}

fn attestation_matches_manifest(
    attestation: &RuntimeAttestation,
    manifest: &EvidenceManifest,
) -> bool {
    attestation.git_head == manifest.expected_clean_revision
        && attestation.tracked_git_status_line_count == 0
        && attestation.embedded_source_bundle_sha256 == manifest.expected_source_bundle_sha256
        && attestation.disk_source_bundle_sha256 == manifest.expected_source_bundle_sha256
        && attestation.cargo_lock_sha256 == manifest.expected_cargo_lock_sha256
        && attestation.toolchain_sha256 == manifest.expected_toolchain_sha256
        && attestation.binary_sha256 == manifest.expected_executable_sha256
        && attestation.rustc_verbose_sha256 == manifest.expected_rustc_verbose_sha256
        && attestation.cargo_build_command_identity
            == manifest.expected_cargo_build_command_identity
        && attestation.rejected_build_override_contract_sha256
            == manifest.expected_rejected_build_override_contract_sha256
        && attestation.build_override_state_sha256 == manifest.expected_build_override_state_sha256
        && attestation.build_override_present_count == 0
}

fn tasklist_processes(repository_root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let output = command_output_at(repository_root, "tasklist", &["/fo", "csv", "/nh"])?;
    Ok(output
        .lines()
        .filter_map(|line| line.split("\",\"").next())
        .map(|field| field.trim_matches('"').trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect())
}

fn blocked_process_category(name: &str) -> Option<&'static str> {
    if matches!(name, "cargo.exe" | "rustc.exe" | "cl.exe" | "link.exe") {
        Some("rust_build_toolchain")
    } else if matches!(name, "python.exe" | "pythonw.exe") {
        Some("python_runtime")
    } else if (name.starts_with("bench_") || name.contains("benchmark"))
        && name != "bench_fast_sampler.exe"
    {
        Some("benchmark_like")
    } else {
        None
    }
}

fn summarize_processes(mut processes: Vec<String>) -> ProcessProbe {
    processes.sort();
    let normalized_inventory_sha256 = sha256_hex(processes.join("\n").as_bytes());
    let mut blocked = Vec::new();
    let mut counts = BlockedProcessCategoryCounts::default();
    for name in &processes {
        if let Some(category) = blocked_process_category(name) {
            match category {
                "rust_build_toolchain" => counts.rust_build_toolchain += 1,
                "python_runtime" => counts.python_runtime += 1,
                "benchmark_like" => counts.benchmark_like += 1,
                _ => unreachable!("fixed process categories are exhaustive"),
            }
            blocked.push(name.as_str());
        }
    }
    let blocked_process_count = counts.total();
    let normalized_blocked_inventory_sha256 = sha256_hex(blocked.join("\n").as_bytes());
    ProcessProbe {
        valid: true,
        observed_process_count: processes.len(),
        blocked_category_counts: counts,
        blocked_process_count,
        normalized_inventory_sha256,
        normalized_blocked_inventory_sha256,
    }
}

fn failed_process_probe() -> ProcessProbe {
    ProcessProbe {
        valid: false,
        observed_process_count: 0,
        blocked_category_counts: BlockedProcessCategoryCounts::default(),
        blocked_process_count: 0,
        normalized_inventory_sha256: sha256_hex(b"process-probe-error"),
        normalized_blocked_inventory_sha256: sha256_hex(b"process-probe-error"),
    }
}

fn is_classified_gpu_workload(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "python.exe",
        "pythonw.exe",
        "bench_",
        "benchmark",
        "gpu-burn",
        "gpuburn",
        "furmark",
        "3dmark",
        "occt",
        "blender.exe",
        "train.exe",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

fn gpu_inventory() -> GpuInventory {
    const POLICY: &str = "non_gating_inventory_only; endpoint process categories and per-repeat external CPU accounting are the interference gates; WDDM visibility is incomplete";
    let output = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,process_name",
            "--format=csv,noheader,nounits",
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut lines = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>();
            lines.sort_unstable();
            GpuInventory {
                probe_status: "valid_non_gating",
                reported_compute_process_count: lines.len(),
                classified_compute_or_benchmark_count: lines
                    .iter()
                    .filter(|line| is_classified_gpu_workload(line))
                    .count(),
                unidentified_process_count: lines
                    .iter()
                    .filter(|line| line.contains("[Insufficient Permissions]"))
                    .count(),
                normalized_inventory_sha256: sha256_hex(lines.join("\n").as_bytes()),
                gate_policy: POLICY,
            }
        }
        Ok(_) => GpuInventory {
            probe_status: "command_error_non_gating",
            reported_compute_process_count: 0,
            classified_compute_or_benchmark_count: 0,
            unidentified_process_count: 0,
            normalized_inventory_sha256: sha256_hex(b"gpu-command-error"),
            gate_policy: POLICY,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => GpuInventory {
            probe_status: "unavailable_non_gating",
            reported_compute_process_count: 0,
            classified_compute_or_benchmark_count: 0,
            unidentified_process_count: 0,
            normalized_inventory_sha256: sha256_hex(b"gpu-unavailable"),
            gate_policy: POLICY,
        },
        Err(_) => GpuInventory {
            probe_status: "probe_error_non_gating",
            reported_compute_process_count: 0,
            classified_compute_or_benchmark_count: 0,
            unidentified_process_count: 0,
            normalized_inventory_sha256: sha256_hex(b"gpu-probe-error"),
            gate_policy: POLICY,
        },
    }
}

fn interference_snapshot(repository_root: &Path) -> InterferenceSnapshot {
    let process = tasklist_processes(repository_root)
        .map(summarize_processes)
        .unwrap_or_else(|_| failed_process_probe());
    InterferenceSnapshot {
        process,
        gpu_inventory: gpu_inventory(),
    }
}

fn profile_histogram_statistics(
    histogram: &[WidthHistogramEntry],
) -> Result<WidthStatistics, Box<dyn Error>> {
    if histogram.is_empty() {
        return Err("width profile histogram is empty".into());
    }
    let mut previous_width = 0;
    let mut sample_count = 0_u64;
    let mut weighted_sum = 0_u128;
    for entry in histogram {
        if entry.width <= previous_width
            || !(1..=64).contains(&entry.width)
            || entry.policy_decision_count == 0
        {
            return Err("width profile histogram is not strictly sorted and admitted".into());
        }
        previous_width = entry.width;
        sample_count = sample_count
            .checked_add(entry.policy_decision_count)
            .ok_or("width profile sample count overflow")?;
        weighted_sum = weighted_sum
            .checked_add(entry.width as u128 * u128::from(entry.policy_decision_count))
            .ok_or("width profile weighted sum overflow")?;
    }
    let target = sample_count
        .checked_mul(95)
        .and_then(|value| value.checked_add(99))
        .ok_or("width profile percentile target overflow")?
        / 100;
    let mut cumulative = 0_u64;
    let mut nearest_rank_p95 = 0;
    for entry in histogram {
        cumulative += entry.policy_decision_count;
        if cumulative >= target {
            nearest_rank_p95 = entry.width;
            break;
        }
    }
    Ok(WidthStatistics {
        sample_count,
        mean: weighted_sum as f64 / sample_count as f64,
        nearest_rank_p95,
        maximum: histogram.last().expect("nonempty histogram").width,
    })
}

fn validate_width_profile(profile: &WorkloadWidthProfile) -> Result<(), Box<dyn Error>> {
    let calculated = profile_histogram_statistics(&profile.histogram)?;
    if calculated.sample_count != profile.statistics.sample_count
        || calculated.nearest_rank_p95 != profile.statistics.nearest_rank_p95
        || calculated.maximum != profile.statistics.maximum
        || (calculated.mean - profile.statistics.mean).abs() > 1e-12
    {
        return Err("width profile statistics do not match its histogram".into());
    }
    match profile.claim_eligible {
        true => {
            if profile.status != OBSERVED_PROFILE_STATUS
                || profile.scope != OBSERVED_PROFILE_SCOPE
                || profile.source_artifact.is_none()
                || profile.source_schema.is_none()
                || profile
                    .source_artifact_sha256
                    .as_deref()
                    .is_none_or(|value| !is_lower_hex(value, 64))
                || profile
                    .source_aggregate_record_sha256
                    .as_deref()
                    .is_none_or(|value| !is_lower_hex(value, 64))
                || profile.source_provenance_class.as_deref()
                    != Some("deterministic_workload_shape_only_not_performance_evidence")
                || profile.source_performance_gate_valid != Some(false)
                || profile.source_performance_rates_included != Some(false)
                || profile.source_coverage_scope.as_deref()
                    != Some("rally_vs_rally_only_not_nine_deck_coverage")
                || profile.raw_source_artifact_sha256.as_deref()
                    != Some("682198c7e169a67a2c885dd8362db0c67c329b8cb1e6390f4fbc905c3f9bd7ee")
                || profile.raw_source_artifact_size_bytes != Some(64_453)
                || profile.source_head_before.as_deref() != Some(RALLY_SOURCE_HEAD)
                || profile.source_head_after != profile.source_head_before
                || profile.source_worktree_state_before.as_deref() != Some("clean")
                || profile.source_worktree_state_after.as_deref() != Some("clean")
                || profile.source_status_sha256_before.as_deref() != Some(EMPTY_SHA256)
                || profile.source_status_sha256_after != profile.source_status_sha256_before
                || profile.source_manifest_file_count != Some(132)
                || profile.source_manifest_sha256_before.as_deref()
                    != Some(RALLY_SOURCE_MANIFEST_SHA256)
                || profile.source_manifest_sha256_after != profile.source_manifest_sha256_before
                || profile.bound_build_source_manifest_sha256_before
                    != profile.source_manifest_sha256_before
                || profile.bound_build_source_manifest_sha256_after
                    != profile.source_manifest_sha256_before
                || profile.source_attestations_stable != Some(true)
                || profile.environment_binary_sha256_prebuild.as_deref()
                    != Some(RALLY_ENVIRONMENT_BINARY_SHA256)
                || profile.environment_binary_sha256_postbuild
                    != profile.environment_binary_sha256_prebuild
                || profile.environment_binary_sha256_before
                    != profile.environment_binary_sha256_prebuild
                || profile.environment_binary_sha256_after
                    != profile.environment_binary_sha256_prebuild
                || profile.benchmark_binary_sha256_prebuild.as_deref()
                    != Some(RALLY_BENCHMARK_BINARY_SHA256)
                || profile.benchmark_binary_sha256_postbuild
                    != profile.benchmark_binary_sha256_prebuild
                || profile.benchmark_binary_sha256_before
                    != profile.benchmark_binary_sha256_prebuild
                || profile.benchmark_binary_sha256_after != profile.benchmark_binary_sha256_prebuild
                || profile.binary_attestations_stable != Some(true)
                || profile.formal_binary_source_attestation_present != Some(false)
                || profile.compiled_input_closure_attested != Some(false)
            {
                return Err("claim-eligible width profile lacks exact observed provenance".into());
            }
            validate_repo_relative_path(
                profile
                    .source_artifact
                    .as_deref()
                    .expect("checked observed source path"),
            )?;
        }
        false => {
            if profile.status != PROVISIONAL_PROFILE_STATUS
                || profile.scope != PROVISIONAL_PROFILE_SCOPE
                || profile.source_artifact.is_some()
                || profile.source_artifact_sha256.is_some()
                || profile.source_schema.is_some()
                || profile.source_aggregate_record_sha256.is_some()
                || profile.source_provenance_class.is_some()
                || profile.source_performance_gate_valid.is_some()
                || profile.source_performance_rates_included.is_some()
                || profile.source_coverage_scope.is_some()
                || profile.raw_source_artifact_sha256.is_some()
                || profile.raw_source_artifact_size_bytes.is_some()
                || profile.source_head_before.is_some()
                || profile.source_head_after.is_some()
                || profile.source_worktree_state_before.is_some()
                || profile.source_worktree_state_after.is_some()
                || profile.source_status_sha256_before.is_some()
                || profile.source_status_sha256_after.is_some()
                || profile.source_manifest_file_count.is_some()
                || profile.source_manifest_sha256_before.is_some()
                || profile.source_manifest_sha256_after.is_some()
                || profile.bound_build_source_manifest_sha256_before.is_some()
                || profile.bound_build_source_manifest_sha256_after.is_some()
                || profile.source_attestations_stable.is_some()
                || profile.environment_binary_sha256_prebuild.is_some()
                || profile.environment_binary_sha256_postbuild.is_some()
                || profile.environment_binary_sha256_before.is_some()
                || profile.environment_binary_sha256_after.is_some()
                || profile.benchmark_binary_sha256_prebuild.is_some()
                || profile.benchmark_binary_sha256_postbuild.is_some()
                || profile.benchmark_binary_sha256_before.is_some()
                || profile.benchmark_binary_sha256_after.is_some()
                || profile.binary_attestations_stable.is_some()
                || profile.formal_binary_source_attestation_present.is_some()
                || profile.compiled_input_closure_attested.is_some()
            {
                return Err(
                    "nonclaiming width profile must use the exact provisional contract".into(),
                );
            }
        }
    }
    if profile.final_all_nine_deck_gate != "deferred" {
        return Err("all-nine-deck sampler gate must remain explicitly deferred".into());
    }
    Ok(())
}

fn load_workload() -> Result<Workload, Box<dyn Error>> {
    let oracle: OracleFile = serde_json::from_slice(ORACLE)?;
    if oracle.schema_version != 2 {
        return Err("fast sampler oracle schema 2 is required".into());
    }
    validate_width_profile(&oracle.workload_width_profile)?;
    let expected_histogram = oracle
        .workload_width_profile
        .histogram
        .iter()
        .map(|entry| (entry.width, entry.policy_decision_count))
        .collect::<BTreeMap<_, _>>();
    let mut actual_histogram = BTreeMap::new();
    let mut weighted_cases = Vec::new();
    let mut cumulative = 0_u64;
    for case in oracle.cases {
        if case.width_profile_count == 0 {
            continue;
        }
        if case.classification != "width-profile-representative" {
            return Err("weighted workload case has a non-profile classification".into());
        }
        let logits = case
            .logit_bits_hex
            .iter()
            .map(|encoded| {
                if encoded.len() != 8 {
                    return Err("workload logit encoding is not eight lowercase hex digits");
                }
                let bits = u32::from_str_radix(encoded, 16)
                    .map_err(|_| "workload logit encoding is invalid")?;
                let value = f32::from_bits(bits);
                if !value.is_finite() {
                    return Err("workload contains a nonfinite logit");
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !(1..=64).contains(&logits.len()) {
            return Err("weighted workload width is outside 1..64".into());
        }
        if actual_histogram
            .insert(logits.len(), case.width_profile_count)
            .is_some()
        {
            return Err("width profile has duplicate representative cases".into());
        }
        cumulative = cumulative
            .checked_add(case.width_profile_count)
            .ok_or("workload cumulative count overflow")?;
        weighted_cases.push(WeightedCase {
            upper_bound: cumulative,
            logits,
        });
    }
    if actual_histogram != expected_histogram
        || cumulative != oracle.workload_width_profile.statistics.sample_count
    {
        return Err("weighted cases do not exactly match the width profile histogram".into());
    }
    Ok(Workload {
        profile: oracle.workload_width_profile,
        cases: weighted_cases,
        total_weight: cumulative,
    })
}

fn observed_source_artifact_digest(
    repository_root: &Path,
    profile: &WorkloadWidthProfile,
) -> Result<Option<String>, Box<dyn Error>> {
    if !profile.claim_eligible {
        return Ok(None);
    }
    let source = profile
        .source_artifact
        .as_deref()
        .ok_or("claim-eligible workload has no source artifact")?;
    let relative = validate_repo_relative_path(source)?;
    let canonical = ensure_existing_path_within_root(repository_root, &relative)?;
    let digest = sha256_hex(&fs::read(canonical)?);
    if profile.source_artifact_sha256.as_deref() != Some(digest.as_str()) {
        return Err("observed width evidence bytes do not match the oracle binding".into());
    }
    Ok(Some(digest))
}

#[cfg(windows)]
fn pin_current_thread(target_cpu: usize) -> Result<u32, String> {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
        fn GetCurrentProcessorNumber() -> u32;
    }

    if target_cpu >= usize::BITS as usize {
        return Err("affinity-target-out-of-mask-range".to_owned());
    }
    let previous = unsafe { SetThreadAffinityMask(GetCurrentThread(), 1_usize << target_cpu) };
    if previous == 0 {
        return Err("set-thread-affinity-mask-failed".to_owned());
    }
    for _ in 0..10_000 {
        let observed = unsafe { GetCurrentProcessorNumber() };
        if observed == target_cpu as u32 {
            return Ok(observed);
        }
        thread::yield_now();
    }
    Err("affinity-observation-mismatch".to_owned())
}

#[cfg(not(windows))]
fn pin_current_thread(_target_cpu: usize) -> Result<u32, String> {
    Err("thread-affinity-unsupported".to_owned())
}

#[cfg(windows)]
fn cpu_time_snapshot() -> Result<CpuTimeSnapshot, &'static str> {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }

    fn ticks(value: FileTime) -> u64 {
        u64::from(value.low) | (u64::from(value.high) << 32)
    }

    let mut idle = FileTime::default();
    let mut system_kernel = FileTime::default();
    let mut system_user = FileTime::default();
    if unsafe { GetSystemTimes(&mut idle, &mut system_kernel, &mut system_user) } == 0 {
        return Err("get-system-times-failed");
    }
    let mut creation = FileTime::default();
    let mut exit = FileTime::default();
    let mut process_kernel = FileTime::default();
    let mut process_user = FileTime::default();
    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut process_kernel,
            &mut process_user,
        )
    } == 0
    {
        return Err("get-process-times-failed");
    }
    Ok(CpuTimeSnapshot {
        system_idle_ticks: ticks(idle),
        system_kernel_ticks: ticks(system_kernel),
        system_user_ticks: ticks(system_user),
        process_kernel_ticks: ticks(process_kernel),
        process_user_ticks: ticks(process_user),
    })
}

#[cfg(not(windows))]
fn cpu_time_snapshot() -> Result<CpuTimeSnapshot, &'static str> {
    Err("cpu-accounting-unsupported")
}

fn invalid_cpu_accounting(
    start_alignment_slack_ns: u64,
    end_alignment_slack_ns: u64,
    error_code: &'static str,
) -> CpuAccounting {
    CpuAccounting {
        probe_valid: false,
        arithmetic_valid: false,
        alignment_valid: false,
        start_alignment_slack_ns,
        end_alignment_slack_ns,
        system_total_capacity_ticks: None,
        system_busy_ticks: None,
        benchmark_process_ticks: None,
        external_busy_ticks: None,
        external_busy_fraction_of_total_capacity: None,
        external_busy_fraction_within_bound: false,
        error_code: Some(error_code),
    }
}

fn calculate_cpu_accounting(
    before: CpuTimeSnapshot,
    after: CpuTimeSnapshot,
    start_alignment_slack_ns: u64,
    end_alignment_slack_ns: u64,
) -> CpuAccounting {
    let arithmetic = (|| {
        let idle = after
            .system_idle_ticks
            .checked_sub(before.system_idle_ticks)?;
        let kernel = after
            .system_kernel_ticks
            .checked_sub(before.system_kernel_ticks)?;
        let user = after
            .system_user_ticks
            .checked_sub(before.system_user_ticks)?;
        let process_kernel = after
            .process_kernel_ticks
            .checked_sub(before.process_kernel_ticks)?;
        let process_user = after
            .process_user_ticks
            .checked_sub(before.process_user_ticks)?;
        let total = kernel.checked_add(user)?;
        let busy = total.checked_sub(idle)?;
        let process = process_kernel.checked_add(process_user)?;
        let external = busy.checked_sub(process)?;
        if total == 0 {
            return None;
        }
        Some((total, busy, process, external))
    })();
    let alignment_valid = start_alignment_slack_ns <= MAXIMUM_CPU_ALIGNMENT_SLACK_NS
        && end_alignment_slack_ns <= MAXIMUM_CPU_ALIGNMENT_SLACK_NS;
    let Some((total, busy, process, external)) = arithmetic else {
        return CpuAccounting {
            probe_valid: true,
            arithmetic_valid: false,
            alignment_valid,
            start_alignment_slack_ns,
            end_alignment_slack_ns,
            system_total_capacity_ticks: None,
            system_busy_ticks: None,
            benchmark_process_ticks: None,
            external_busy_ticks: None,
            external_busy_fraction_of_total_capacity: None,
            external_busy_fraction_within_bound: false,
            error_code: Some("cpu-accounting-arithmetic-invalid"),
        };
    };
    let fraction = external as f64 / total as f64;
    CpuAccounting {
        probe_valid: true,
        arithmetic_valid: true,
        alignment_valid,
        start_alignment_slack_ns,
        end_alignment_slack_ns,
        system_total_capacity_ticks: Some(total),
        system_busy_ticks: Some(busy),
        benchmark_process_ticks: Some(process),
        external_busy_ticks: Some(external),
        external_busy_fraction_of_total_capacity: Some(fraction),
        external_busy_fraction_within_bound: alignment_valid
            && fraction <= MAXIMUM_EXTERNAL_CPU_BUSY_FRACTION,
        error_code: None,
    }
}

fn duration_ns_u64(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

fn worker(
    worker: usize,
    workload: Arc<Workload>,
    ready: Arc<Barrier>,
    schedule: Arc<OnceLock<Schedule>>,
) -> WorkerResult {
    let affinity = pin_current_thread(worker);
    let affinity_valid = affinity.is_ok();
    ready.wait();
    let timing = loop {
        if let Some(timing) = schedule.get() {
            break *timing;
        }
        spin_loop();
    };
    let actual_start = Instant::now();
    let mut scratch = FastCategoricalScratch::default();
    let mut decisions = 0_u64;
    let mut checksum = 0_u64;
    let mut error_code = affinity.as_ref().err().cloned();
    while error_code.is_none() {
        for _ in 0..CHUNK_DECISIONS {
            let ordinal = decisions.wrapping_add((worker as u64).wrapping_mul(17));
            let logits = workload.logits_for(ordinal);
            let seed = decisions
                .wrapping_mul(0xd134_2543_de82_ef95)
                .wrapping_add((worker as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
                .wrapping_add(0xa076_1d64_78bd_642f);
            match scratch.sample(black_box(logits), black_box(seed)) {
                Ok(selected) => {
                    checksum = checksum.rotate_left(7) ^ selected as u64 ^ seed;
                    decisions += 1;
                }
                Err(error) => {
                    error_code = Some(format!("sampler-{error}"));
                    break;
                }
            }
        }
        if Instant::now() >= timing.deadline {
            break;
        }
    }
    black_box(checksum);
    let finish = Instant::now();
    WorkerResult {
        worker,
        target_cpu: worker,
        observed_cpu: affinity.ok(),
        affinity_valid,
        start_delay_ns: duration_ns_u64(actual_start.saturating_duration_since(timing.start)),
        finish_overshoot_ns: duration_ns_u64(finish.saturating_duration_since(timing.deadline)),
        finish_offset_ns: duration_ns_u64(finish.saturating_duration_since(timing.start)),
        decisions,
        selection_checksum: checksum,
        error_code,
    }
}

fn run_repeat(
    threads: usize,
    repeat: usize,
    duration_ms: u64,
    workload: Arc<Workload>,
) -> Result<RepeatResult, Box<dyn Error>> {
    let ready = Arc::new(Barrier::new(threads + 1));
    let schedule = Arc::new(OnceLock::new());
    let handles = (0..threads)
        .map(|worker_index| {
            let worker_workload = Arc::clone(&workload);
            let worker_ready = Arc::clone(&ready);
            let worker_schedule = Arc::clone(&schedule);
            thread::spawn(move || {
                worker(worker_index, worker_workload, worker_ready, worker_schedule)
            })
        })
        .collect::<Vec<_>>();
    ready.wait();

    let before_probe_started = Instant::now();
    let before_cpu = cpu_time_snapshot();
    let start = Instant::now();
    let deadline = start + Duration::from_millis(duration_ms);
    schedule
        .set(Schedule { start, deadline })
        .map_err(|_| "benchmark schedule was initialized twice")?;

    let mut workers = handles
        .into_iter()
        .map(|handle| handle.join().map_err(|_| "benchmark worker panicked"))
        .collect::<Result<Vec<_>, _>>()?;
    workers.sort_by_key(|worker| worker.worker);
    let common_wall_clock_ns = workers
        .iter()
        .map(|worker| worker.finish_offset_ns)
        .max()
        .ok_or("benchmark produced no workers")?;
    let common_finish = start + Duration::from_nanos(common_wall_clock_ns);
    let after_cpu = cpu_time_snapshot();
    let after_probe_finished = Instant::now();
    let start_alignment_slack_ns =
        duration_ns_u64(start.saturating_duration_since(before_probe_started));
    let end_alignment_slack_ns =
        duration_ns_u64(after_probe_finished.saturating_duration_since(common_finish));
    let cpu_accounting = match (before_cpu, after_cpu) {
        (Ok(before), Ok(after)) => calculate_cpu_accounting(
            before,
            after,
            start_alignment_slack_ns,
            end_alignment_slack_ns,
        ),
        (Err(error), _) | (_, Err(error)) => {
            invalid_cpu_accounting(start_alignment_slack_ns, end_alignment_slack_ns, error)
        }
    };

    let aggregate_decisions = workers.iter().map(|worker| worker.decisions).sum::<u64>();
    let decisions_per_second =
        aggregate_decisions as f64 * 1_000_000_000.0 / common_wall_clock_ns as f64;
    let maximum_start_delay_ns = workers
        .iter()
        .map(|worker| worker.start_delay_ns)
        .max()
        .unwrap_or(u64::MAX);
    let maximum_finish_overshoot_ns = workers
        .iter()
        .map(|worker| worker.finish_overshoot_ns)
        .max()
        .unwrap_or(u64::MAX);
    let minimum_worker_decisions = workers
        .iter()
        .map(|worker| worker.decisions)
        .min()
        .unwrap_or(0);
    let maximum_worker_decisions = workers
        .iter()
        .map(|worker| worker.decisions)
        .max()
        .unwrap_or(0);
    let affinity_valid = workers.iter().all(|worker| {
        worker.affinity_valid && worker.observed_cpu == Some(worker.target_cpu as u32)
    });
    let timing_valid = maximum_start_delay_ns <= MAXIMUM_START_DELAY_NS
        && maximum_finish_overshoot_ns <= MAXIMUM_OVERSHOOT_NS;
    Ok(RepeatResult {
        repeat,
        threads,
        requested_duration_ms: duration_ms,
        common_wall_clock_ns,
        aggregate_decisions,
        decisions_per_second,
        maximum_start_delay_ns,
        maximum_finish_overshoot_ns,
        minimum_worker_decisions,
        maximum_worker_decisions,
        affinity_valid,
        timing_valid,
        cpu_accounting,
        workers,
    })
}

fn warm_up(workload: &Workload) -> Result<u64, Box<dyn Error>> {
    let mut scratch = FastCategoricalScratch::default();
    let mut checksum = 0_u64;
    for decision in 0..100_000_u64 {
        checksum ^= scratch
            .sample(workload.logits_for(decision), decision)?
            .rotate_left((decision % 31) as u32) as u64;
    }
    Ok(black_box(checksum))
}

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, Box<dyn Error>> {
    fn write_value(value: &Value, output: &mut Vec<u8>) -> Result<(), Box<dyn Error>> {
        match value {
            Value::Null => output.extend_from_slice(b"null"),
            Value::Bool(boolean) => {
                output.extend_from_slice(if *boolean { b"true" } else { b"false" })
            }
            Value::Number(number) => output.extend_from_slice(number.to_string().as_bytes()),
            Value::String(string) => serde_json::to_writer(output, string)?,
            Value::Array(values) => {
                output.push(b'[');
                for (index, item) in values.iter().enumerate() {
                    if index != 0 {
                        output.push(b',');
                    }
                    write_value(item, output)?;
                }
                output.push(b']');
            }
            Value::Object(values) => {
                output.push(b'{');
                let mut entries = values.iter().collect::<Vec<_>>();
                entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
                for (index, (key, item)) in entries.into_iter().enumerate() {
                    if index != 0 {
                        output.push(b',');
                    }
                    serde_json::to_writer(&mut *output, key)?;
                    output.push(b':');
                    write_value(item, output)?;
                }
                output.push(b'}');
            }
        }
        Ok(())
    }

    let mut output = Vec::new();
    write_value(value, &mut output)?;
    Ok(output)
}

fn evidence_envelope(validation_record: Value) -> Result<(Value, String), Box<dyn Error>> {
    let validation_record_sha256 = sha256_hex(&canonical_json_bytes(&validation_record)?);
    let envelope = json!({
        "schema_version": EVIDENCE_SCHEMA,
        "validation_record_hash_contract": "SHA-256 of UTF-8 canonical JSON: object keys sorted lexicographically, no insignificant whitespace, arrays retain order",
        "validation_record_sha256": validation_record_sha256,
        "validation_record": validation_record,
    });
    let digest = envelope["validation_record_sha256"]
        .as_str()
        .ok_or("evidence digest field is not a string")?
        .to_owned();
    Ok((envelope, digest))
}

fn publish_bytes_atomic_no_replace(
    repository_root: &Path,
    output_relative: &Path,
    bytes: &[u8],
) -> Result<(), Box<dyn Error>> {
    validate_output_target(repository_root, output_relative)?;
    let output = repository_root.join(output_relative);
    let parent = output.parent().ok_or("evidence output has no parent")?;
    let file_name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("evidence filename is not UTF-8")?;
    let nonce = PUBLICATION_NONCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".{file_name}.tmp.{}.{}", std::process::id(), nonce));
    let mut temporary_created = false;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        temporary_created = true;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        fs::hard_link(&temporary, &output)
            .map_err(|error| format!("atomic no-replace hard-link publication failed: {error}"))?;
        if fs::read(&output)? != bytes {
            return Err("published evidence failed exact byte readback verification".into());
        }
        Ok(())
    })();
    if temporary_created {
        if let Err(cleanup_error) = fs::remove_file(&temporary) {
            return Err(format!("temporary evidence cleanup failed: {cleanup_error}").into());
        }
    }
    result
}

fn write_evidence_atomic(
    repository_root: &Path,
    output_relative: &Path,
    envelope: &Value,
) -> Result<(), Box<dyn Error>> {
    let mut bytes = serde_json::to_vec_pretty(envelope)?;
    bytes.push(b'\n');
    publish_bytes_atomic_no_replace(repository_root, output_relative, &bytes)
}

fn throughput_summaries(repeats: &[RepeatResult]) -> Vec<Value> {
    [1, 16]
        .into_iter()
        .map(|threads| {
            let mut values = repeats
                .iter()
                .filter(|repeat| repeat.threads == threads)
                .map(|repeat| repeat.decisions_per_second)
                .collect::<Vec<_>>();
            values.sort_by(f64::total_cmp);
            let minimum = values.first().copied().unwrap_or(0.0);
            let median = values.get(values.len() / 2).copied().unwrap_or(0.0);
            let maximum = values.last().copied().unwrap_or(0.0);
            json!({
                "threads": threads,
                "repeat_count": values.len(),
                "decisions_per_second": values,
                "minimum": minimum,
                "median": median,
                "maximum": maximum,
                "minimum_target_multiple": minimum / TARGET_DECISIONS_PER_SECOND,
            })
        })
        .collect()
}

fn run() -> Result<bool, Box<dyn Error>> {
    let arguments = parse_arguments()?;
    let repository_root = repository_root()?;
    if let (Some(manifest_path), Some(evidence_output_path)) = (
        arguments.write_manifest_template.as_deref(),
        arguments.template_evidence_output.as_deref(),
    ) {
        write_manifest_template_workflow(&repository_root, manifest_path, evidence_output_path)?;
        return Ok(true);
    }
    let canonical_parameters =
        arguments.repeats == CANONICAL_REPEATS && arguments.duration_ms == CANONICAL_DURATION_MS;
    let loaded_manifest = arguments
        .evidence_manifest
        .as_deref()
        .map(|path| load_manifest(&repository_root, path))
        .transpose()?;
    if let Some(loaded) = &loaded_manifest {
        let output_relative = validate_manifest_fields(&loaded.manifest)?;
        validate_output_target(&repository_root, &output_relative)?;
    }

    let workload = Arc::new(load_workload()?);
    let observed_workload_claim_eligible = workload.profile.claim_eligible
        && workload.profile.status == OBSERVED_PROFILE_STATUS
        && workload.profile.scope == OBSERVED_PROFILE_SCOPE;
    if canonical_parameters && !observed_workload_claim_eligible {
        return Err("canonical benchmark refused: width histogram is provisional_synthetic; provenance-bound all-policy Rally evidence is required".into());
    }

    let observed_source_artifact_before =
        observed_source_artifact_digest(&repository_root, &workload.profile)?;

    let warmup_checksum = warm_up(&workload)?;
    let before_attestation = runtime_attestation(&repository_root)?;
    let before_interference = interference_snapshot(&repository_root);

    let mut repeats = Vec::with_capacity(arguments.repeats * 2);
    for threads in [1, 16] {
        for repeat in 0..arguments.repeats {
            repeats.push(run_repeat(
                threads,
                repeat,
                arguments.duration_ms,
                Arc::clone(&workload),
            )?);
        }
    }

    let after_interference = interference_snapshot(&repository_root);
    let after_attestation = runtime_attestation(&repository_root)?;
    let observed_source_artifact_after =
        observed_source_artifact_digest(&repository_root, &workload.profile)?;
    let manifest_stable = if let Some(loaded) = &loaded_manifest {
        let relative = validate_repo_relative_path(&loaded.repo_relative_path)?;
        let canonical = ensure_existing_path_within_root(&repository_root, &relative)?;
        sha256_hex(&fs::read(canonical)?) == loaded.bytes_sha256
    } else {
        true
    };
    let source_binding_valid = before_attestation.embedded_source_bundle_sha256
        == before_attestation.disk_source_bundle_sha256
        && after_attestation.embedded_source_bundle_sha256
            == after_attestation.disk_source_bundle_sha256;
    let provenance_stable = before_attestation == after_attestation;
    let toolchain_pinned = before_attestation.rustc_release == "1.94.1"
        && String::from_utf8_lossy(TOOLCHAIN).contains("channel = \"1.94.1\"");
    let manifest_binding_valid = loaded_manifest.as_ref().is_some_and(|loaded| {
        attestation_matches_manifest(&before_attestation, &loaded.manifest)
            && attestation_matches_manifest(&after_attestation, &loaded.manifest)
    });
    let executable_binding_valid = loaded_manifest.as_ref().is_some_and(|loaded| {
        before_attestation.binary_sha256 == loaded.manifest.expected_executable_sha256
            && after_attestation.binary_sha256 == loaded.manifest.expected_executable_sha256
            && before_attestation.binary_sha256 == after_attestation.binary_sha256
    });
    let rustc_verbose_binding_valid = loaded_manifest.as_ref().is_some_and(|loaded| {
        before_attestation.rustc_verbose_sha256 == loaded.manifest.expected_rustc_verbose_sha256
            && after_attestation.rustc_verbose_sha256
                == loaded.manifest.expected_rustc_verbose_sha256
            && before_attestation.rustc_verbose_sha256 == after_attestation.rustc_verbose_sha256
    });
    let build_identity_binding_valid = loaded_manifest.as_ref().is_some_and(|loaded| {
        before_attestation.cargo_build_command_identity
            == loaded.manifest.expected_cargo_build_command_identity
            && after_attestation.cargo_build_command_identity
                == loaded.manifest.expected_cargo_build_command_identity
            && before_attestation.rejected_build_override_contract_sha256
                == loaded
                    .manifest
                    .expected_rejected_build_override_contract_sha256
            && after_attestation.rejected_build_override_contract_sha256
                == loaded
                    .manifest
                    .expected_rejected_build_override_contract_sha256
            && before_attestation.build_override_state_sha256
                == loaded.manifest.expected_build_override_state_sha256
            && after_attestation.build_override_state_sha256
                == loaded.manifest.expected_build_override_state_sha256
            && before_attestation.build_override_present_count == 0
            && after_attestation.build_override_present_count == 0
    });
    let observed_source_artifact_stable = observed_source_artifact_before.is_some()
        && observed_source_artifact_before == observed_source_artifact_after;
    let endpoint_process_interference_valid = before_interference.process.valid
        && after_interference.process.valid
        && before_interference.process.blocked_process_count == 0
        && after_interference.process.blocked_process_count == 0;
    let affinity_valid = repeats.iter().all(|repeat| repeat.affinity_valid);
    let timing_valid = repeats.iter().all(|repeat| repeat.timing_valid);
    let cpu_accounting_valid = repeats.iter().all(|repeat| {
        repeat.cpu_accounting.probe_valid
            && repeat.cpu_accounting.arithmetic_valid
            && repeat.cpu_accounting.alignment_valid
    });
    let external_cpu_interference_valid = repeats
        .iter()
        .all(|repeat| repeat.cpu_accounting.external_busy_fraction_within_bound);
    let throughput_valid = repeats
        .iter()
        .all(|repeat| repeat.decisions_per_second >= TARGET_DECISIONS_PER_SECOND);
    let worker_errors_absent = repeats
        .iter()
        .flat_map(|repeat| &repeat.workers)
        .all(|worker| worker.error_code.is_none());
    let hardware_admitted = before_attestation.available_parallelism >= 16;
    let claim_valid = canonical_parameters
        && observed_workload_claim_eligible
        && source_binding_valid
        && provenance_stable
        && manifest_stable
        && manifest_binding_valid
        && executable_binding_valid
        && rustc_verbose_binding_valid
        && build_identity_binding_valid
        && observed_source_artifact_stable
        && toolchain_pinned
        && endpoint_process_interference_valid
        && affinity_valid
        && timing_valid
        && cpu_accounting_valid
        && external_cpu_interference_valid
        && throughput_valid
        && worker_errors_absent
        && hardware_admitted;

    let summaries = throughput_summaries(&repeats);
    if !canonical_parameters {
        println!(
            "{}",
            serde_json::to_string(&json!({
                "schema_version": "fast_sampler_benchmark_diagnostic_summary/v1",
                "status": "diagnostic_only_noncanonical",
                "canonical_evidence_written": false,
                "parameters": {"repeats": arguments.repeats, "duration_ms": arguments.duration_ms},
                "workload_status": workload.profile.status,
                "workload_claim_eligible": workload.profile.claim_eligible,
                "summaries": summaries,
            }))?
        );
        return Ok(true);
    }

    let claim_checks = json!({
        "canonical_parameters": canonical_parameters,
        "observed_workload_claim_eligible": observed_workload_claim_eligible,
        "source_binding_valid": source_binding_valid,
        "provenance_stable": provenance_stable,
        "manifest_stable": manifest_stable,
        "manifest_binding_valid": manifest_binding_valid,
        "executable_binding_valid": executable_binding_valid,
        "rustc_verbose_binding_valid": rustc_verbose_binding_valid,
        "build_identity_binding_valid": build_identity_binding_valid,
        "observed_source_artifact_stable": observed_source_artifact_stable,
        "toolchain_pinned": toolchain_pinned,
        "endpoint_process_interference_valid": endpoint_process_interference_valid,
        "affinity_valid": affinity_valid,
        "timing_valid": timing_valid,
        "cpu_accounting_valid": cpu_accounting_valid,
        "external_cpu_interference_valid": external_cpu_interference_valid,
        "throughput_valid": throughput_valid,
        "worker_errors_absent": worker_errors_absent,
        "hardware_admitted": hardware_admitted,
    });
    if !claim_valid {
        println!(
            "{}",
            serde_json::to_string(&json!({
                "schema_version": "fast_sampler_benchmark_failed_summary/v1",
                "status": "fail_closed",
                "canonical_evidence_written": false,
                "claim_checks": claim_checks,
                "summaries": summaries,
            }))?
        );
        return Ok(false);
    }

    let loaded = loaded_manifest
        .as_ref()
        .expect("canonical arguments require a loaded manifest");
    let output_relative = validate_manifest_fields(&loaded.manifest)?;
    let validation_record = json!({
        "schema_version": VALIDATION_RECORD_SCHEMA,
        "privacy_contract": "no host paths and no process names; process and GPU observations are fixed category counts plus normalized digests",
        "benchmark": "complete quantize + Q63 lookup + bounded u128 Hamilton apportionment with checked release postconditions + SplitMix64 first draw + inverse-CDF selected index",
        "parameters": {
            "thread_counts": [1, 16],
            "repeats": arguments.repeats,
            "duration_ms": arguments.duration_ms,
            "target_decisions_per_second": TARGET_DECISIONS_PER_SECOND,
            "maximum_start_delay_ns": MAXIMUM_START_DELAY_NS,
            "maximum_overshoot_ns": MAXIMUM_OVERSHOOT_NS,
            "maximum_cpu_alignment_slack_ns": MAXIMUM_CPU_ALIGNMENT_SLACK_NS,
            "maximum_external_cpu_busy_fraction_of_total_capacity": MAXIMUM_EXTERNAL_CPU_BUSY_FRACTION,
        },
        "timing_contract": "GetSystemTimes/GetProcessTimes snapshots bracket the common coordinator start through latest worker finish; system busy minus benchmark process CPU is divided by total system capacity; alignment slack is recorded and gated for every repeat",
        "workload_width_profile": workload.profile,
        "observed_source_artifact_sha256": observed_source_artifact_before,
        "warmup_checksum": warmup_checksum,
        "manifest_binding": {
            "manifest_repo_relative_path": loaded.repo_relative_path,
            "manifest_sha256": loaded.bytes_sha256,
            "expected_clean_revision": loaded.manifest.expected_clean_revision,
            "expected_source_bundle_sha256": loaded.manifest.expected_source_bundle_sha256,
            "expected_cargo_lock_sha256": loaded.manifest.expected_cargo_lock_sha256,
            "expected_toolchain_sha256": loaded.manifest.expected_toolchain_sha256,
            "expected_executable_sha256": loaded.manifest.expected_executable_sha256,
            "expected_rustc_verbose_sha256": loaded.manifest.expected_rustc_verbose_sha256,
            "expected_cargo_build_command_identity": loaded.manifest.expected_cargo_build_command_identity,
            "expected_rejected_build_override_contract_sha256": loaded.manifest.expected_rejected_build_override_contract_sha256,
            "expected_build_override_state_sha256": loaded.manifest.expected_build_override_state_sha256,
            "evidence_output_path": loaded.manifest.evidence_output_path,
        },
        "attestation": {
            "before": before_attestation,
            "after": after_attestation,
        },
        "interference": {
            "endpoint_before": before_interference,
            "endpoint_after": after_interference,
            "gpu_inventory_gating": false,
        },
        "claim_checks": claim_checks,
        "summaries": summaries,
        "repeats": repeats,
        "positive_sampler_microbenchmark_claim_valid": true,
        "canonical_observed_workload_claim_valid": true,
        "final_all_nine_deck_sampler_gate": "deferred",
        "claim_boundary": "sampler microbenchmark only; excludes model inference, environment, IPC, training, all-nine-deck completion, and any learning noninferiority claim",
        "atomic_write_contract": "same-directory create_new temporary file, write_all, flush, sync_all, atomic hard_link to the destination as a no-replace primitive, exact destination byte readback, then mandatory temporary cleanup",
    });
    let (envelope, validation_record_sha256) = evidence_envelope(validation_record)?;
    write_evidence_atomic(&repository_root, &output_relative, &envelope)?;
    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema_version": "fast_sampler_benchmark_pass_summary/v1",
            "status": "pass",
            "canonical_evidence_written": true,
            "evidence_output_path": output_relative.to_string_lossy().replace('\\', "/"),
            "validation_record_sha256": validation_record_sha256,
        }))?
    );
    Ok(true)
}

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("FAST_SAMPLER_BENCHMARK: FAIL_CLOSED");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("FAST_SAMPLER_BENCHMARK: ERROR: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    static TEMPORARY_DIRECTORY_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn valid_manifest() -> EvidenceManifest {
        EvidenceManifest {
            schema_version: MANIFEST_SCHEMA.to_owned(),
            expected_clean_revision: "0".repeat(40),
            expected_source_bundle_sha256: "1".repeat(64),
            expected_cargo_lock_sha256: "2".repeat(64),
            expected_toolchain_sha256: "3".repeat(64),
            expected_executable_sha256: "4".repeat(64),
            expected_rustc_verbose_sha256: "5".repeat(64),
            expected_cargo_build_command_identity: CARGO_BUILD_COMMAND_IDENTITY.to_owned(),
            expected_rejected_build_override_contract_sha256: sha256_hex(
                REJECTED_BUILD_OVERRIDE_CONTRACT.as_bytes(),
            ),
            expected_build_override_state_sha256: EMPTY_SHA256.to_owned(),
            evidence_output_path: "evidence/fast_sampler/run-001.json".to_owned(),
        }
    }

    fn temporary_root(label: &str) -> PathBuf {
        let unique = TEMPORARY_DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "mtg-kernel-fast-sampler-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::canonicalize(root).unwrap()
    }

    fn matching_attestation(manifest: &EvidenceManifest) -> RuntimeAttestation {
        RuntimeAttestation {
            binary_sha256: manifest.expected_executable_sha256.clone(),
            embedded_source_bundle_sha256: manifest.expected_source_bundle_sha256.clone(),
            disk_source_bundle_sha256: manifest.expected_source_bundle_sha256.clone(),
            cargo_lock_sha256: manifest.expected_cargo_lock_sha256.clone(),
            toolchain_sha256: manifest.expected_toolchain_sha256.clone(),
            git_head: manifest.expected_clean_revision.clone(),
            tracked_git_status_sha256: EMPTY_SHA256.to_owned(),
            tracked_git_status_line_count: 0,
            tracked_git_diff_sha256: EMPTY_SHA256.to_owned(),
            rustc_verbose_sha256: manifest.expected_rustc_verbose_sha256.clone(),
            rustc_release: "1.94.1".to_owned(),
            rustc_host: "x86_64-pc-windows-msvc".to_owned(),
            cargo_build_command_identity: manifest.expected_cargo_build_command_identity.clone(),
            rejected_build_override_contract_sha256: manifest
                .expected_rejected_build_override_contract_sha256
                .clone(),
            build_override_state_sha256: manifest.expected_build_override_state_sha256.clone(),
            build_override_present_count: 0,
            processor_identifier: "test-processor".to_owned(),
            available_parallelism: 16,
            number_of_processors: "16".to_owned(),
            os_version_sha256: "6".repeat(64),
            active_power_scheme_sha256: "7".repeat(64),
        }
    }

    #[test]
    fn repo_relative_paths_fail_closed() {
        assert!(validate_repo_relative_path("evidence/fast_sampler/run.json").is_ok());
        for rejected in [
            "",
            "/absolute.json",
            "C:/outside.json",
            "evidence\\outside.json",
            "evidence/../outside.json",
            "evidence//outside.json",
            ".git/config",
        ] {
            assert!(validate_repo_relative_path(rejected).is_err(), "{rejected}");
        }
    }

    #[test]
    fn evidence_manifest_shape_and_hashes_are_strict() {
        assert!(validate_manifest_fields(&valid_manifest()).is_ok());
        let mut invalid = valid_manifest();
        invalid.expected_clean_revision = "A".repeat(40);
        assert!(validate_manifest_fields(&invalid).is_err());
        let mut invalid = valid_manifest();
        invalid.evidence_output_path = "data/result.json".to_owned();
        assert!(validate_manifest_fields(&invalid).is_err());
        let mut invalid = valid_manifest();
        invalid.expected_executable_sha256 = "0".repeat(63);
        assert!(validate_manifest_fields(&invalid).is_err());
        let mut invalid = valid_manifest();
        invalid.expected_cargo_build_command_identity = "cargo build --release".to_owned();
        assert!(validate_manifest_fields(&invalid).is_err());
        let mut invalid = valid_manifest();
        invalid.expected_build_override_state_sha256 = "f".repeat(64);
        assert!(validate_manifest_fields(&invalid).is_err());
    }

    #[test]
    fn manifest_binding_covers_executable_compiler_and_build_contract() {
        let manifest = valid_manifest();
        let attestation = matching_attestation(&manifest);
        assert!(attestation_matches_manifest(&attestation, &manifest));
        let generated = manifest_from_attestation(
            &attestation,
            "evidence/fast_sampler/generated-evidence.json",
        );
        assert_eq!(
            generated.expected_executable_sha256,
            attestation.binary_sha256
        );
        assert_eq!(
            generated.expected_rustc_verbose_sha256,
            attestation.rustc_verbose_sha256
        );
        assert!(validate_manifest_fields(&generated).is_ok());

        let mut drifted = attestation.clone();
        drifted.binary_sha256 = "8".repeat(64);
        assert!(!attestation_matches_manifest(&drifted, &manifest));
        let mut drifted = attestation.clone();
        drifted.rustc_verbose_sha256 = "9".repeat(64);
        assert!(!attestation_matches_manifest(&drifted, &manifest));
        let mut drifted = attestation.clone();
        drifted.cargo_build_command_identity = "different build".to_owned();
        assert!(!attestation_matches_manifest(&drifted, &manifest));
        let mut drifted = attestation;
        drifted.build_override_present_count = 1;
        assert!(!attestation_matches_manifest(&drifted, &manifest));
    }

    #[test]
    fn rejected_build_override_contract_is_explicit() {
        for name in [
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "RUSTC_WRAPPER",
            "CARGO_PROFILE_RELEASE_LTO",
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER",
        ] {
            assert!(rejected_build_override_name(name), "{name}");
        }
        assert!(!rejected_build_override_name("CARGO_HOME"));
        assert_eq!(sha256_hex(&[]), EMPTY_SHA256);
    }

    #[test]
    fn manifest_template_paths_and_directory_creation_fail_closed() {
        assert!(validate_evidence_child_json("evidence/fast_sampler/run.json").is_ok());
        for rejected in [
            "evidence/fast_sampler/nested/run.json",
            "evidence/fast_sampler/run.txt",
            "evidence/../run.json",
        ] {
            assert!(
                validate_evidence_child_json(rejected).is_err(),
                "{rejected}"
            );
        }

        let root = temporary_root("safe-evidence-directory");
        let prepared = safe_prepare_evidence_directory(&root).unwrap();
        assert_eq!(prepared, root.join(EVIDENCE_DIRECTORY));
        fs::remove_dir_all(&root).unwrap();

        let root = temporary_root("blocked-evidence-directory");
        fs::write(root.join("evidence"), b"not a directory").unwrap();
        assert!(safe_prepare_evidence_directory(&root).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn process_evidence_never_serializes_process_names() {
        let probe = summarize_processes(vec![
            "python.exe".to_owned(),
            "private-customer-benchmark.exe".to_owned(),
            "unrelated-secret.exe".to_owned(),
        ]);
        assert_eq!(probe.blocked_category_counts.python_runtime, 1);
        assert_eq!(probe.blocked_category_counts.benchmark_like, 1);
        let serialized = serde_json::to_string(&probe).unwrap();
        for private_name in [
            "python.exe",
            "private-customer-benchmark.exe",
            "unrelated-secret.exe",
        ] {
            assert!(!serialized.contains(private_name));
        }
    }

    #[test]
    fn external_cpu_accounting_subtracts_benchmark_process_cpu() {
        let before = CpuTimeSnapshot {
            system_idle_ticks: 100,
            system_kernel_ticks: 400,
            system_user_ticks: 200,
            process_kernel_ticks: 10,
            process_user_ticks: 20,
        };
        let after = CpuTimeSnapshot {
            system_idle_ticks: 300,
            system_kernel_ticks: 1_000,
            system_user_ticks: 600,
            process_kernel_ticks: 110,
            process_user_ticks: 220,
        };
        let accounting = calculate_cpu_accounting(before, after, 1_000, 2_000);
        assert_eq!(accounting.system_total_capacity_ticks, Some(1_000));
        assert_eq!(accounting.system_busy_ticks, Some(800));
        assert_eq!(accounting.benchmark_process_ticks, Some(300));
        assert_eq!(accounting.external_busy_ticks, Some(500));
        assert_eq!(
            accounting.external_busy_fraction_of_total_capacity,
            Some(0.5)
        );
        assert!(!accounting.external_busy_fraction_within_bound);
    }

    #[test]
    fn committed_fixture_binds_observed_workload_but_not_source_performance() {
        let workload = load_workload().unwrap();
        assert_eq!(workload.profile.status, OBSERVED_PROFILE_STATUS);
        assert!(workload.profile.claim_eligible);
        assert_eq!(workload.profile.source_performance_gate_valid, Some(false));
        assert_eq!(
            workload.profile.source_performance_rates_included,
            Some(false)
        );
        assert_eq!(workload.profile.final_all_nine_deck_gate, "deferred");
        let root = repository_root().unwrap();
        assert_eq!(
            observed_source_artifact_digest(&root, &workload.profile).unwrap(),
            Some("d9471ee78ee8b656040d1920118f962f4b239e55603220e3679b1d11b847e579".to_owned())
        );
    }

    #[test]
    fn validation_record_hash_is_canonical_and_recomputable() {
        let record = json!({"z": [3, 2, 1], "a": {"y": true, "x": false}});
        let expected = sha256_hex(br#"{"a":{"x":false,"y":true},"z":[3,2,1]}"#);
        let (envelope, digest) = evidence_envelope(record).unwrap();
        assert_eq!(digest, expected);
        assert_eq!(envelope["validation_record_sha256"], expected);
    }

    #[test]
    fn evidence_publication_never_overwrites_a_preexisting_destination() {
        let root = temporary_root("evidence-collision");
        fs::create_dir_all(root.join("evidence/fast_sampler")).unwrap();
        let relative = Path::new("evidence/fast_sampler/result.json");
        let sentinel = b"preexisting-sentinel\n";
        fs::write(root.join(relative), sentinel).unwrap();
        let (envelope, _) = evidence_envelope(json!({"test": true})).unwrap();
        assert!(write_evidence_atomic(&root, relative, &envelope).is_err());
        assert_eq!(fs::read(root.join(relative)).unwrap(), sentinel);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn adversarial_publication_race_has_exactly_one_complete_winner() {
        let root = temporary_root("evidence-race");
        fs::create_dir_all(root.join("evidence/fast_sampler")).unwrap();
        let relative = PathBuf::from("evidence/fast_sampler/result.json");
        let payloads = [vec![b'a'; 64 * 1024], vec![b'b'; 64 * 1024]];
        let barrier = Arc::new(Barrier::new(payloads.len()));
        let workers = payloads
            .iter()
            .cloned()
            .map(|payload| {
                let root = root.clone();
                let relative = relative.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    publish_bytes_atomic_no_replace(&root, &relative, &payload)
                        .map(|()| payload)
                        .map_err(|error| error.to_string())
                })
            })
            .collect::<Vec<_>>();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let winner = results.into_iter().find_map(Result::ok).unwrap();
        assert_eq!(fs::read(root.join(relative)).unwrap(), winner);
        let temporary_count = fs::read_dir(root.join(EVIDENCE_DIRECTORY))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
            .count();
        assert_eq!(temporary_count, 0);
        fs::remove_dir_all(&root).unwrap();
    }
}
