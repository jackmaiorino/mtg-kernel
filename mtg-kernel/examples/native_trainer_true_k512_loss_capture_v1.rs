//! Provenance-bound capture for the native trainer's true-K=512 scalar-loss gate.
//!
//! The executable runs one genuine, non-cycled Rally/Rally production update and
//! writes every physical loss input plus the production sequential reductions.
//! It does not run Torch or decide the cross-language gate; the separately pinned
//! Python authority consumes this capture. The output must be outside the source
//! repository so clean-source preflight and postflight can be compared exactly.

use mtg_kernel::native_training_executor_v1::{
    native_training_episode_schedule_v1, NativeTrainingExecutionConfigV1,
    NativeTrainingExecutorErrorV1, NativeTrainingExecutorV1, NativeTrainingNumericalBackendV1,
    NativeTrainingPhaseProfileV1, NativeTrainingPhaseV1, NativeTrainingSnapshotReceiptV1,
    NativeTrainingUpdateObservationV2, NATIVE_TRAINING_NUMERICAL_BACKEND_IDENTITY_V1,
};
use mtg_kernel::rl::{PlayerSeatV1, TerminalOutcomeV1};
use mtg_kernel::strict_source_tree_attestation_v1::{
    capture_strict_source_tree_v1, require_strict_source_postflight_equality_v1,
    require_strict_source_preflight_v1, StrictSourceTreeAttestationErrorV1,
    StrictSourceTreeCaptureV1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const SCHEMA: &str = "native-policy-loss-reduction-true-k512-capture-v1";
const IDENTITY: &str = "native-production-sequential-genuine-rally-k512-v1";
const CAPTURE_HARNESS_PATH: &str =
    "mtg-kernel/examples/native_trainer_true_k512_loss_capture_v1.rs";
const TERM_STREAM_FRAMING: &str = "for group_index in production order: u64be(group_index)||u32be(joint_log_probability_f32_bits)||u32be(value_f32_bits)||i8_twos_complement(terminal_return)";
const SELECTED_STREAM_FRAMING: &str = "for selected output in production order: u64be(group_index)||u64be(substep_index)||u64be(selected_action_index)||u32be(selected_logit_f32_bits)||u32be(value_f32_bits)||u32be(selected_log_probability_f32_bits)";
const EPISODE_STREAM_FRAMING: &str = "for episode ordinal in production order: u64be(ordinal)||u64be(episode_index)||u64be(environment_seed)||u64be(deck_hash_p0)||u64be(deck_hash_p1)||u8(learner_seat)||i8_twos_complement(learner_return)||u64be(learner_group_count)||u64be(learner_policy_step_count)||raw32(full_trajectory_sha256)";
const RUN_BASE_SEED: u64 = 71_501;
const K: u64 = 512;
const PAIR_COUNT: usize = 256;
const ENVIRONMENT_SEED_OCCURRENCES_PER_PAIR: usize = 2;
const DECK_ID: &str = "Rally";
const MAX_PHYSICAL_DECISIONS: u64 = 5_000;
const MAX_POLICY_STEPS: u64 = 640_000;
const SCHEDULER_TIMEOUT: Duration = Duration::from_secs(600);
const VALUE_COEFFICIENT: f32 = 0.5;
const LEARNING_RATE: f32 = 0.001;

const USAGE: &str = "\
Native trainer genuine Rally K=512 scalar-loss capture\n\
\n\
Usage:\n\
  cargo run -p mtg-kernel --release --example native_trainer_true_k512_loss_capture_v1 -- \\\n\
    --workers <positive integer> --sessions-per-worker <positive integer> \\\n\
    --broker-target <positive integer> --output <outside-repository-path> \\\n\
    --expected-source-commit <40-lower-hex> --expected-source-tree <64-lower-hex> \\\n\
    [--snapshot-manifest <path> --snapshot-payload <path>]\n\
\n\
The workload is fixed: one fresh-snapshot Rally/Rally update, K=512, run seed\n\
71501, value coefficient 0.5, and learning rate 0.001. The source worktree must\n\
be clean and match the expected commit/tree before and after the update.\n";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Args {
    workers: usize,
    sessions_per_worker: usize,
    broker_target: usize,
    output: PathBuf,
    expected_source_commit: String,
    expected_source_tree: String,
    snapshot_manifest: PathBuf,
    snapshot_payload: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
enum ParseOutcome {
    Run(Args),
    Help,
}

#[derive(Default)]
struct RawArgs {
    workers: Option<usize>,
    sessions_per_worker: Option<usize>,
    broker_target: Option<usize>,
    output: Option<PathBuf>,
    expected_source_commit: Option<String>,
    expected_source_tree: Option<String>,
    snapshot_manifest: Option<PathBuf>,
    snapshot_payload: Option<PathBuf>,
    help: bool,
    non_help_argument_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppError(String);

impl AppError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    fn executor(context: &'static str, error: NativeTrainingExecutorErrorV1) -> Self {
        Self::new(format!(
            "{context}: native executor rejected the operation ({})",
            error.code()
        ))
    }

    fn source_attestation(
        context: &'static str,
        error: StrictSourceTreeAttestationErrorV1,
    ) -> Self {
        Self::new(format!("{context}: {}", error.code()))
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AppError {}

#[derive(Serialize)]
struct CaptureRecord {
    schema: &'static str,
    identity: &'static str,
    nonclaim: &'static str,
    source: SourceRecord,
    workload: WorkloadRecord,
    snapshot: SnapshotRecord,
    sizing_row: SizingRow,
    episodes: EpisodeStreamRecord,
    selected_outputs: SelectedStreamRecord,
    term_stream: TermStreamRecord,
    rust_production_reduction: RustReductionRecord,
}

#[derive(Serialize)]
struct SourceRecord {
    strict_source_tree: StrictSourceTreeCaptureV1,
    preflight_validated: bool,
    postflight_equality_validated: bool,
    executable_sha256: String,
    capture_harness_sha256: String,
    capture_harness_path: &'static str,
}

#[derive(Serialize)]
struct WorkloadRecord {
    composition_identity: &'static str,
    composition_nonclaim: &'static str,
    trainer_contract_identity: &'static str,
    numerical_backend_identity: &'static str,
    run_base_seed: u64,
    batch_episodes: u64,
    deck_ids: [&'static str; 2],
    max_physical_decisions: u64,
    max_policy_steps: u64,
    worker_count: usize,
    sessions_per_worker: usize,
    logical_actor_count: usize,
    broker_batch_target: usize,
    scheduler_timeout_ms: u64,
    measure_broker_service_time: bool,
    value_coefficient_f32_bits: String,
    learning_rate_f32_bits: String,
}

#[derive(Serialize)]
struct SnapshotRecord {
    schema: String,
    identity: String,
    snapshot_sha256: String,
    manifest_file_sha256: String,
    manifest_core_sha256: String,
    payload_sha256: String,
    payload_byte_count: u64,
    parameter_layout_sha256: String,
    named_parameter_stream_sha256: String,
    loaded_named_parameter_stream_sha256: String,
    model_config_fingerprint: String,
    model_architecture_version: String,
    feature_contract_digest: String,
    feature_encoding_digest: String,
    initializer_identity: String,
    base_seed: u64,
    model_init_seed: u64,
    trainer_schedule_version: String,
    python_reference_seed_version: String,
    schedule_goldens_sha256: String,
    authority_source_bundle_sha256: String,
    authority_runtime_identity: String,
    loader_identity: String,
    optimizer_identity: String,
    adam_step_initial: u64,
    scorer_bias_anchor_f32_bits: u64,
}

#[derive(Serialize)]
struct SizingRow {
    update_ordinal: u64,
    outer_update_elapsed_ns: u128,
    executor_update_elapsed_ns: u64,
    rollout_elapsed_ns: u64,
    episode_count: u64,
    physical_decision_count: u64,
    policy_step_count: u64,
    learner_group_count: u64,
    learner_policy_step_count: u64,
    scorer_accepted_batch_count: u64,
    scorer_accepted_decision_count: u64,
    scored_action_logit_count: u64,
    model_digest_before: String,
    model_digest_after: String,
    changed_non_gauge_parameter_count: usize,
    adam_step_before: u64,
    adam_step_after: u64,
}

#[derive(Serialize)]
struct EpisodeStreamRecord {
    framing: &'static str,
    sha256: String,
    independent_episode_count: usize,
    distinct_environment_seed_count: usize,
    environment_seed_occurrences_per_distinct_seed: usize,
    each_environment_seed_occurs_exactly_twice: bool,
    consecutive_even_odd_seed_pairing_validated: bool,
    environment_seeds_recomputed_from_frozen_schedule: bool,
    learner_seats_recomputed_from_frozen_schedule: bool,
    records: Vec<EpisodeRecord>,
}

struct EpisodeScheduleProof {
    distinct_environment_seed_count: usize,
    each_environment_seed_occurs_exactly_twice: bool,
    consecutive_even_odd_seed_pairing_validated: bool,
    environment_seeds_recomputed_from_frozen_schedule: bool,
    learner_seats_recomputed_from_frozen_schedule: bool,
}

#[derive(Serialize)]
struct EpisodeRecord {
    ordinal: usize,
    episode_index: u64,
    environment_seed: u64,
    deck_hashes: [u64; 2],
    learner_seat: &'static str,
    learner_return: i8,
    terminal_outcome: &'static str,
    learner_group_count: u64,
    learner_policy_step_count: u64,
    term_begin_inclusive: usize,
    term_end_exclusive: usize,
    full_trajectory_sha256: String,
    full_policy_step_count: u64,
    full_physical_decision_count: u64,
    opponent_policy_step_count: u64,
    opponent_physical_decision_count: u64,
}

#[derive(Serialize)]
struct SelectedStreamRecord {
    framing: &'static str,
    sha256: String,
    count: usize,
}

#[derive(Serialize)]
struct TermStreamRecord {
    framing: &'static str,
    sha256: String,
    learner_physical_decision_group_count: usize,
    policy_term_count: usize,
    value_term_count: usize,
    policy_nonzero_count: usize,
    value_nonzero_count: usize,
    terminal_return_counts: [usize; 3],
    terms: Vec<TermRecord>,
}

#[derive(Serialize)]
struct TermRecord {
    group_index: usize,
    joint_log_probability_f32_bits: String,
    value_f32_bits: String,
    terminal_return: i8,
}

struct BuiltTermStream {
    records: Vec<TermRecord>,
    sha256: String,
    policy_nonzero_count: usize,
    value_nonzero_count: usize,
    terminal_return_counts: [usize; 3],
}

#[derive(Serialize)]
struct RustReductionRecord {
    operation: &'static str,
    reconstruction_matches_production_bits: bool,
    policy_sum: ScalarRecord,
    value_sum: ScalarRecord,
    loss: ScalarRecord,
}

#[derive(Serialize)]
struct ScalarRecord {
    value: f64,
    f32_bits: String,
}

fn main() -> ExitCode {
    let parsed = match parse_args(env::args_os().skip(1)) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("Argument error: {error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match parsed {
        ParseOutcome::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        ParseOutcome::Run(args) => match run_capture(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("K512 loss capture failed: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

fn parse_args(arguments: impl IntoIterator<Item = OsString>) -> Result<ParseOutcome, AppError> {
    let mut raw = RawArgs::default();
    let mut arguments = arguments.into_iter();
    while let Some(flag) = arguments.next() {
        let flag = flag
            .into_string()
            .map_err(|_| AppError::new("argument names must be valid Unicode"))?;
        if flag == "--help" {
            if raw.help {
                return Err(AppError::new("--help may appear only once"));
            }
            raw.help = true;
            continue;
        }
        raw.non_help_argument_count = raw
            .non_help_argument_count
            .checked_add(1)
            .ok_or_else(|| AppError::new("argument count overflow"))?;
        match flag.as_str() {
            "--workers" => set_once(
                &mut raw.workers,
                parse_usize(next_value(&mut arguments, "--workers")?, "--workers")?,
                "--workers",
            )?,
            "--sessions-per-worker" => set_once(
                &mut raw.sessions_per_worker,
                parse_usize(
                    next_value(&mut arguments, "--sessions-per-worker")?,
                    "--sessions-per-worker",
                )?,
                "--sessions-per-worker",
            )?,
            "--broker-target" => set_once(
                &mut raw.broker_target,
                parse_usize(
                    next_value(&mut arguments, "--broker-target")?,
                    "--broker-target",
                )?,
                "--broker-target",
            )?,
            "--output" => set_once(
                &mut raw.output,
                PathBuf::from(next_value(&mut arguments, "--output")?),
                "--output",
            )?,
            "--expected-source-commit" => set_once(
                &mut raw.expected_source_commit,
                parse_lower_hex(
                    next_value(&mut arguments, "--expected-source-commit")?,
                    "--expected-source-commit",
                    40,
                )?,
                "--expected-source-commit",
            )?,
            "--expected-source-tree" => set_once(
                &mut raw.expected_source_tree,
                parse_lower_hex(
                    next_value(&mut arguments, "--expected-source-tree")?,
                    "--expected-source-tree",
                    64,
                )?,
                "--expected-source-tree",
            )?,
            "--snapshot-manifest" => set_once(
                &mut raw.snapshot_manifest,
                PathBuf::from(next_value(&mut arguments, "--snapshot-manifest")?),
                "--snapshot-manifest",
            )?,
            "--snapshot-payload" => set_once(
                &mut raw.snapshot_payload,
                PathBuf::from(next_value(&mut arguments, "--snapshot-payload")?),
                "--snapshot-payload",
            )?,
            _ => return Err(AppError::new(format!("unknown argument: {flag}"))),
        }
    }
    if raw.help {
        if raw.non_help_argument_count == 0 {
            return Ok(ParseOutcome::Help);
        }
        return Err(AppError::new(
            "--help cannot be combined with run arguments",
        ));
    }
    let workers = positive(required(raw.workers, "--workers")?, "--workers")?;
    let sessions_per_worker = positive(
        required(raw.sessions_per_worker, "--sessions-per-worker")?,
        "--sessions-per-worker",
    )?;
    let broker_target = positive(
        required(raw.broker_target, "--broker-target")?,
        "--broker-target",
    )?;
    let logical_actor_count = workers
        .checked_mul(sessions_per_worker)
        .ok_or_else(|| AppError::new("workers times sessions-per-worker overflows usize"))?;
    if broker_target > logical_actor_count {
        return Err(AppError::new(
            "--broker-target must not exceed workers times sessions-per-worker",
        ));
    }
    let (snapshot_manifest, snapshot_payload) = match (raw.snapshot_manifest, raw.snapshot_payload)
    {
        (Some(manifest), Some(payload)) => (manifest, payload),
        (None, None) => repository_snapshot_paths()?,
        _ => {
            return Err(AppError::new(
                "--snapshot-manifest and --snapshot-payload must be supplied together",
            ))
        }
    };
    Ok(ParseOutcome::Run(Args {
        workers,
        sessions_per_worker,
        broker_target,
        output: required(raw.output, "--output")?,
        expected_source_commit: required(raw.expected_source_commit, "--expected-source-commit")?,
        expected_source_tree: required(raw.expected_source_tree, "--expected-source-tree")?,
        snapshot_manifest,
        snapshot_payload,
    }))
}

fn next_value(
    arguments: &mut impl Iterator<Item = OsString>,
    flag: &'static str,
) -> Result<OsString, AppError> {
    let value = arguments
        .next()
        .ok_or_else(|| AppError::new(format!("{flag} requires a value")))?;
    if value.is_empty() || value.to_str().is_some_and(|text| text.starts_with("--")) {
        return Err(AppError::new(format!("{flag} requires a nonempty value")));
    }
    Ok(value)
}

fn parse_usize(value: OsString, flag: &'static str) -> Result<usize, AppError> {
    value
        .into_string()
        .map_err(|_| AppError::new(format!("{flag} requires an ASCII unsigned integer")))?
        .parse::<usize>()
        .map_err(|_| AppError::new(format!("{flag} requires an ASCII unsigned integer")))
}

fn parse_lower_hex(value: OsString, flag: &'static str, length: usize) -> Result<String, AppError> {
    let value = value
        .into_string()
        .map_err(|_| AppError::new(format!("{flag} requires {length} lowercase hex characters")))?;
    if value.len() != length
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(AppError::new(format!(
            "{flag} requires {length} lowercase hex characters"
        )));
    }
    Ok(value)
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &'static str) -> Result<(), AppError> {
    if slot.replace(value).is_some() {
        Err(AppError::new(format!("{flag} may appear only once")))
    } else {
        Ok(())
    }
}

fn required<T>(value: Option<T>, flag: &'static str) -> Result<T, AppError> {
    value.ok_or_else(|| AppError::new(format!("missing required argument {flag}")))
}

fn positive(value: usize, flag: &'static str) -> Result<usize, AppError> {
    if value == 0 {
        Err(AppError::new(format!("{flag} must be positive")))
    } else {
        Ok(value)
    }
}

fn repository_root() -> Result<PathBuf, AppError> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| AppError::new("crate directory has no repository parent"))
        .and_then(|path| {
            path.canonicalize()
                .map_err(|_| AppError::new("repository root cannot be canonicalized"))
        })
}

fn repository_snapshot_paths() -> Result<(PathBuf, PathBuf), AppError> {
    let directory = repository_root()?
        .join("data")
        .join("common_model_snapshot_v1");
    Ok((
        directory.join("manifest.json"),
        directory.join("parameters.f32le"),
    ))
}

fn validate_external_output_path(repo_root: &Path, output: &Path) -> Result<PathBuf, AppError> {
    if output.file_name().is_none() {
        return Err(AppError::new("--output must name a file"));
    }
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()
        .map_err(|_| AppError::new("--output parent must already exist"))?;
    let canonical_repo = repo_root
        .canonicalize()
        .map_err(|_| AppError::new("repository root cannot be canonicalized"))?;
    if parent.starts_with(&canonical_repo) {
        return Err(AppError::new(
            "--output must be outside the source repository",
        ));
    }
    let resolved = parent.join(
        output
            .file_name()
            .ok_or_else(|| AppError::new("--output must name a file"))?,
    );
    if resolved.exists() {
        return Err(AppError::new("--output already exists"));
    }
    Ok(resolved)
}

fn run_capture(args: Args) -> Result<(), AppError> {
    let repo_root = repository_root()?;
    let output_path = validate_external_output_path(&repo_root, &args.output)?;
    let source_before = capture_strict_source_tree_v1(&repo_root)
        .map_err(|error| AppError::source_attestation("source preflight capture failed", error))?;
    require_strict_source_preflight_v1(
        &source_before,
        &args.expected_source_commit,
        &args.expected_source_tree,
    )
    .map_err(|error| AppError::source_attestation("source preflight failed", error))?;
    let executable_sha256 = sha256_file(
        &env::current_exe().map_err(|_| AppError::new("current executable unavailable"))?,
    )?;
    let capture_harness_sha256 = sha256_file(&repo_root.join(CAPTURE_HARNESS_PATH))?;
    let logical_actor_count = args
        .workers
        .checked_mul(args.sessions_per_worker)
        .ok_or_else(|| AppError::new("logical actor count overflow"))?;
    let config = NativeTrainingExecutionConfigV1 {
        run_base_seed: RUN_BASE_SEED,
        batch_episodes: K,
        deck_ids: [DECK_ID.to_owned(), DECK_ID.to_owned()],
        max_physical_decisions: MAX_PHYSICAL_DECISIONS,
        max_policy_steps: MAX_POLICY_STEPS,
        worker_count: args.workers,
        sessions_per_worker: args.sessions_per_worker,
        broker_batch_target: args.broker_target,
        scheduler_timeout: SCHEDULER_TIMEOUT,
        measure_broker_service_time: false,
        value_coefficient_bits: VALUE_COEFFICIENT.to_bits(),
        learning_rate_bits: LEARNING_RATE.to_bits(),
        numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
        backward_worker_limit: 1,
    };
    let mut executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
        config,
        &args.snapshot_manifest,
        &args.snapshot_payload,
    )
    .map_err(|error| AppError::executor("snapshot bootstrap failed", error))?;
    let snapshot = executor
        .snapshot_receipt()
        .cloned()
        .ok_or_else(|| AppError::new("fresh executor has no snapshot receipt"))?;
    let progress_before = executor.progress();
    if progress_before.next_episode_index != 0 || progress_before.successful_update_count != 0 {
        return Err(AppError::new("fresh executor progress is not zero"));
    }
    let outer_start = Instant::now();
    let (observation, phase_profile) = executor
        .run_update_with_phase_profile_v1()
        .map_err(|error| AppError::executor("true-K512 update failed", error))?;
    let outer_update_elapsed_ns = outer_start.elapsed().as_nanos();
    emit_phase_profile(&phase_profile, observation.update_elapsed_ns)?;
    let source_after = capture_strict_source_tree_v1(&repo_root)
        .map_err(|error| AppError::source_attestation("source postflight capture failed", error))?;
    require_strict_source_postflight_equality_v1(&source_before, &source_after)
        .map_err(|error| AppError::source_attestation("source postflight failed", error))?;
    let record = build_capture_record(
        source_before,
        executable_sha256,
        capture_harness_sha256,
        logical_actor_count,
        snapshot,
        observation,
        outer_update_elapsed_ns,
    )?;
    let bytes = serde_json::to_vec_pretty(&record)
        .map_err(|_| AppError::new("capture serialization failed"))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_path)
        .map_err(|_| AppError::new("capture output create-new failed"))?;
    output
        .write_all(&bytes)
        .and_then(|_| output.write_all(b"\n"))
        .and_then(|_| output.flush())
        .and_then(|_| output.sync_all())
        .map_err(|_| AppError::new("capture output durable write failed"))?;
    println!(
        "PASS genuine Rally K=512 capture: {}",
        output_path.display()
    );
    Ok(())
}

fn emit_phase_profile(
    profile: &NativeTrainingPhaseProfileV1,
    observation_update_elapsed_ns: u64,
) -> Result<(), AppError> {
    if profile.update_elapsed_ns_v1() != observation_update_elapsed_ns
        || profile.accounted_elapsed_ns_v1() > profile.update_elapsed_ns_v1()
    {
        return Err(AppError::new("native phase profile accounting failed"));
    }
    eprintln!(
        "NATIVE_TRAINER_PHASE_PROFILE_V1 update_elapsed_ns={} accounted_elapsed_ns={} unaccounted_elapsed_ns={}",
        profile.update_elapsed_ns_v1(),
        profile.accounted_elapsed_ns_v1(),
        profile.unaccounted_elapsed_ns_v1(),
    );
    for phase in NativeTrainingPhaseV1::ALL {
        eprintln!(
            "NATIVE_TRAINER_PHASE_PROFILE_V1 phase={} elapsed_ns={} record_count={}",
            phase.label_v1(),
            profile.phase_elapsed_ns_v1(phase),
            profile.phase_record_count_v1(phase),
        );
    }
    for (ordinal, record) in profile.records_v1().iter().enumerate() {
        eprintln!(
            "NATIVE_TRAINER_PHASE_PROFILE_V1 timeline_ordinal={} phase={} elapsed_ns={}",
            ordinal,
            record.phase.label_v1(),
            record.elapsed_ns,
        );
    }
    Ok(())
}

fn build_capture_record(
    source: StrictSourceTreeCaptureV1,
    executable_sha256: String,
    capture_harness_sha256: String,
    logical_actor_count: usize,
    snapshot: NativeTrainingSnapshotReceiptV1,
    observation: NativeTrainingUpdateObservationV2,
    outer_update_elapsed_ns: u128,
) -> Result<CaptureRecord, AppError> {
    validate_observation(&observation)?;
    let (episodes, episode_sha256, episode_schedule_proof) = episode_records(&observation)?;
    let term_stream = term_records(&observation)?;
    let selected_sha256 = selected_stream_sha256(&observation)?;
    let (policy_sum, value_sum, loss) = reconstruct_sequential_reduction(&observation)?;
    let reconstruction_matches_production_bits = policy_sum.to_bits()
        == observation.policy_sum_bits
        && value_sum.to_bits() == observation.value_sum_bits
        && loss.to_bits() == observation.loss_bits;
    if !reconstruction_matches_production_bits {
        return Err(AppError::new(
            "production scalar reconstruction does not match observed bits",
        ));
    }
    Ok(CaptureRecord {
        schema: SCHEMA,
        identity: IDENTITY,
        nonclaim: "single provenance-bound numerical sizing row; not throughput, learning-quality, XMage-parity, or all-deck evidence",
        source: SourceRecord {
            strict_source_tree: source,
            preflight_validated: true,
            postflight_equality_validated: true,
            executable_sha256,
            capture_harness_sha256,
            capture_harness_path: CAPTURE_HARNESS_PATH,
        },
        workload: WorkloadRecord {
            composition_identity: "production-native-training-executor-single-update-genuine-rally-v1",
            composition_nonclaim: "all 512 episodes were independently executed by production NativeTrainingExecutorV1; no physical group or term was cycled, replayed, expanded, or synthetically generated",
            trainer_contract_identity: observation.trainer_contract_identity,
            numerical_backend_identity: NATIVE_TRAINING_NUMERICAL_BACKEND_IDENTITY_V1,
            run_base_seed: RUN_BASE_SEED,
            batch_episodes: K,
            deck_ids: [DECK_ID, DECK_ID],
            max_physical_decisions: MAX_PHYSICAL_DECISIONS,
            max_policy_steps: MAX_POLICY_STEPS,
            worker_count: observation.worker_count,
            sessions_per_worker: observation.sessions_per_worker,
            logical_actor_count,
            broker_batch_target: observation.broker_batch_target,
            scheduler_timeout_ms: u64::try_from(SCHEDULER_TIMEOUT.as_millis())
                .map_err(|_| AppError::new("scheduler timeout does not fit u64"))?,
            measure_broker_service_time: false,
            value_coefficient_f32_bits: f32_bits_hex(VALUE_COEFFICIENT),
            learning_rate_f32_bits: f32_bits_hex(LEARNING_RATE),
        },
        snapshot: snapshot_record(snapshot),
        sizing_row: SizingRow {
            update_ordinal: 0,
            outer_update_elapsed_ns,
            executor_update_elapsed_ns: observation.update_elapsed_ns,
            rollout_elapsed_ns: observation.rollout_metrics.total_elapsed_ns,
            episode_count: observation.episode_count,
            physical_decision_count: observation.physical_decision_count,
            policy_step_count: observation.policy_step_count,
            learner_group_count: observation.learner_group_count,
            learner_policy_step_count: observation.learner_policy_step_count,
            scorer_accepted_batch_count: observation.scorer_accepted_batch_count,
            scorer_accepted_decision_count: observation.scorer_accepted_decision_count,
            scored_action_logit_count: observation.rollout_metrics.scored_action_logit_count,
            model_digest_before: observation.model_digest_before.clone(),
            model_digest_after: observation.model_digest_after.clone(),
            changed_non_gauge_parameter_count: observation.changed_non_gauge_parameter_count,
            adam_step_before: observation.adam_step_before,
            adam_step_after: observation.adam_step_after,
        },
        episodes: EpisodeStreamRecord {
            framing: EPISODE_STREAM_FRAMING,
            sha256: episode_sha256,
            independent_episode_count: episodes.len(),
            distinct_environment_seed_count: episode_schedule_proof
                .distinct_environment_seed_count,
            environment_seed_occurrences_per_distinct_seed:
                ENVIRONMENT_SEED_OCCURRENCES_PER_PAIR,
            each_environment_seed_occurs_exactly_twice: episode_schedule_proof
                .each_environment_seed_occurs_exactly_twice,
            consecutive_even_odd_seed_pairing_validated: episode_schedule_proof
                .consecutive_even_odd_seed_pairing_validated,
            environment_seeds_recomputed_from_frozen_schedule: episode_schedule_proof
                .environment_seeds_recomputed_from_frozen_schedule,
            learner_seats_recomputed_from_frozen_schedule: episode_schedule_proof
                .learner_seats_recomputed_from_frozen_schedule,
            records: episodes,
        },
        selected_outputs: SelectedStreamRecord {
            framing: SELECTED_STREAM_FRAMING,
            sha256: selected_sha256,
            count: observation.selected_outputs.len(),
        },
        term_stream: TermStreamRecord {
            framing: TERM_STREAM_FRAMING,
            sha256: term_stream.sha256,
            learner_physical_decision_group_count: term_stream.records.len(),
            policy_term_count: term_stream.records.len(),
            value_term_count: term_stream.records.len(),
            policy_nonzero_count: term_stream.policy_nonzero_count,
            value_nonzero_count: term_stream.value_nonzero_count,
            terminal_return_counts: term_stream.terminal_return_counts,
            terms: term_stream.records,
        },
        rust_production_reduction: RustReductionRecord {
            operation: "ordered f32 policy_sum += (-joint_log_probability * (terminal_return_f32 - value)); value_sum += ((value - terminal_return_f32) * (value - terminal_return_f32)); loss = (policy_sum + 0.5f32 * value_sum) / learner_group_count_f32",
            reconstruction_matches_production_bits,
            policy_sum: scalar_record(f32::from_bits(observation.policy_sum_bits)),
            value_sum: scalar_record(f32::from_bits(observation.value_sum_bits)),
            loss: scalar_record(f32::from_bits(observation.loss_bits)),
        },
    })
}

fn validate_observation(observation: &NativeTrainingUpdateObservationV2) -> Result<(), AppError> {
    if observation.first_episode_index != 0
        || observation.episode_count != K
        || observation.episodes.len() != K as usize
        || observation.worker_count == 0
        || observation.sessions_per_worker == 0
        || observation.logical_actor_count
            != observation.worker_count * observation.sessions_per_worker
        || observation.broker_batch_target == 0
        || observation.broker_batch_target > observation.logical_actor_count
        || observation.learner_group_count == 0
        || observation.physical_terms.len()
            != usize::try_from(observation.learner_group_count)
                .map_err(|_| AppError::new("learner group count does not fit usize"))?
        || observation.selected_outputs.len()
            != usize::try_from(observation.learner_policy_step_count)
                .map_err(|_| AppError::new("learner policy step count does not fit usize"))?
        || observation.adam_step_before != 0
        || observation.adam_step_after != 1
        || observation.model_digest_before == observation.model_digest_after
        || observation.changed_non_gauge_parameter_count == 0
    {
        return Err(AppError::new(
            "true-K512 production observation invariant failed",
        ));
    }
    Ok(())
}

fn episode_records(
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<(Vec<EpisodeRecord>, String, EpisodeScheduleProof), AppError> {
    let mut digest = Sha256::new();
    let mut records = Vec::with_capacity(observation.episodes.len());
    let mut environment_seed_occurrences: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    let mut consecutive_even_odd_seed_pairing_validated = true;
    let mut environment_seeds_recomputed_from_frozen_schedule = true;
    let mut learner_seats_recomputed_from_frozen_schedule = true;
    let mut term_begin = 0usize;
    for (ordinal, episode) in observation.episodes.iter().enumerate() {
        let expected_index = u64::try_from(ordinal)
            .map_err(|_| AppError::new("episode ordinal does not fit u64"))?;
        let receipt = episode.full_trajectory_receipt;
        let expected_schedule = native_training_episode_schedule_v1(RUN_BASE_SEED, expected_index)
            .map_err(|error| AppError::executor("frozen episode schedule failed", error))?;
        let expected_parity_seat = if ordinal % 2 == 0 {
            PlayerSeatV1::P0
        } else {
            PlayerSeatV1::P1
        };
        environment_seeds_recomputed_from_frozen_schedule &= expected_schedule.episode_index
            == expected_index
            && expected_schedule.pair_index == expected_index / 2
            && receipt.environment_seed == expected_schedule.environment_seed;
        learner_seats_recomputed_from_frozen_schedule &= expected_schedule.learner_seat
            == expected_parity_seat
            && episode.learner_seat == expected_schedule.learner_seat;
        if ordinal % 2 == 1 {
            let previous = observation
                .episodes
                .get(ordinal - 1)
                .ok_or_else(|| AppError::new("paired episode predecessor is missing"))?;
            consecutive_even_odd_seed_pairing_validated &= previous.episode_index.checked_add(1)
                == Some(episode.episode_index)
                && previous.learner_seat == PlayerSeatV1::P0
                && episode.learner_seat == PlayerSeatV1::P1
                && previous.full_trajectory_receipt.environment_seed == receipt.environment_seed;
        }
        if episode.episode_index != expected_index
            || receipt.episode_index != episode.episode_index
            || receipt.learner_seat != episode.learner_seat
            || receipt.learner_policy_step_count != episode.learner_policy_step_count
            || receipt.learner_physical_decision_count != episode.learner_group_count
        {
            return Err(AppError::new("genuine episode provenance invariant failed"));
        }
        environment_seed_occurrences
            .entry(receipt.environment_seed)
            .or_default()
            .push(ordinal);
        let group_count = usize::try_from(episode.learner_group_count)
            .map_err(|_| AppError::new("episode group count does not fit usize"))?;
        let term_end = term_begin
            .checked_add(group_count)
            .ok_or_else(|| AppError::new("episode term range overflow"))?;
        if term_end > observation.physical_terms.len()
            || observation.physical_terms[term_begin..term_end]
                .iter()
                .any(|term| term.terminal_return != episode.learner_return)
        {
            return Err(AppError::new("episode term range/return invariant failed"));
        }
        digest.update(expected_index.to_be_bytes());
        digest.update(episode.episode_index.to_be_bytes());
        digest.update(receipt.environment_seed.to_be_bytes());
        digest.update(receipt.deck_hashes[0].to_be_bytes());
        digest.update(receipt.deck_hashes[1].to_be_bytes());
        digest.update([seat_code(episode.learner_seat)]);
        digest.update([episode.learner_return as u8]);
        digest.update(episode.learner_group_count.to_be_bytes());
        digest.update(episode.learner_policy_step_count.to_be_bytes());
        digest.update(receipt.trajectory_sha256);
        records.push(EpisodeRecord {
            ordinal,
            episode_index: episode.episode_index,
            environment_seed: receipt.environment_seed,
            deck_hashes: receipt.deck_hashes,
            learner_seat: seat_name(episode.learner_seat),
            learner_return: episode.learner_return,
            terminal_outcome: terminal_name(episode.terminal_outcome),
            learner_group_count: episode.learner_group_count,
            learner_policy_step_count: episode.learner_policy_step_count,
            term_begin_inclusive: term_begin,
            term_end_exclusive: term_end,
            full_trajectory_sha256: hex_bytes(&receipt.trajectory_sha256),
            full_policy_step_count: receipt.policy_step_count,
            full_physical_decision_count: receipt.physical_decision_count,
            opponent_policy_step_count: receipt.opponent_policy_step_count,
            opponent_physical_decision_count: receipt.opponent_physical_decision_count,
        });
        term_begin = term_end;
    }
    if term_begin != observation.physical_terms.len() {
        return Err(AppError::new(
            "episode term ranges do not cover term stream",
        ));
    }
    let each_environment_seed_occurs_exactly_twice = environment_seed_occurrences
        .values()
        .all(|ordinals| ordinals.len() == ENVIRONMENT_SEED_OCCURRENCES_PER_PAIR);
    let occurrences_are_consecutive_even_odd =
        environment_seed_occurrences.values().all(|ordinals| {
            ordinals.len() == ENVIRONMENT_SEED_OCCURRENCES_PER_PAIR
                && ordinals[0] % 2 == 0
                && ordinals[1] == ordinals[0] + 1
        });
    consecutive_even_odd_seed_pairing_validated &= occurrences_are_consecutive_even_odd;
    let distinct_environment_seed_count = environment_seed_occurrences.len();
    if distinct_environment_seed_count != PAIR_COUNT
        || !each_environment_seed_occurs_exactly_twice
        || !consecutive_even_odd_seed_pairing_validated
        || !environment_seeds_recomputed_from_frozen_schedule
        || !learner_seats_recomputed_from_frozen_schedule
    {
        return Err(AppError::new(
            "genuine paired episode schedule provenance invariant failed",
        ));
    }
    Ok((
        records,
        hex_bytes(&digest.finalize()),
        EpisodeScheduleProof {
            distinct_environment_seed_count,
            each_environment_seed_occurs_exactly_twice,
            consecutive_even_odd_seed_pairing_validated,
            environment_seeds_recomputed_from_frozen_schedule,
            learner_seats_recomputed_from_frozen_schedule,
        },
    ))
}

fn term_records(
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<BuiltTermStream, AppError> {
    let mut digest = Sha256::new();
    let mut records = Vec::with_capacity(observation.physical_terms.len());
    let mut policy_nonzero_count = 0usize;
    let mut value_nonzero_count = 0usize;
    let mut return_counts = [0usize; 3];
    for (group_index, term) in observation.physical_terms.iter().enumerate() {
        if !matches!(term.terminal_return, -1..=1) {
            return Err(AppError::new("term has invalid terminal return"));
        }
        let joint = f32::from_bits(term.joint_log_probability_bits);
        let value = f32::from_bits(term.value_bits);
        if !joint.is_finite() || !value.is_finite() {
            return Err(AppError::new("term has non-finite input"));
        }
        let target = f32::from(term.terminal_return);
        let policy_term = -joint * (target - value);
        let value_error = value - target;
        let value_term = value_error * value_error;
        policy_nonzero_count += usize::from(policy_term.to_bits() & 0x7fff_ffff != 0);
        value_nonzero_count += usize::from(value_term.to_bits() & 0x7fff_ffff != 0);
        return_counts[usize::try_from(term.terminal_return + 1)
            .map_err(|_| AppError::new("terminal return index failed"))?] += 1;
        let group_index_u64 = u64::try_from(group_index)
            .map_err(|_| AppError::new("group index does not fit u64"))?;
        digest.update(group_index_u64.to_be_bytes());
        digest.update(term.joint_log_probability_bits.to_be_bytes());
        digest.update(term.value_bits.to_be_bytes());
        digest.update([term.terminal_return as u8]);
        records.push(TermRecord {
            group_index,
            joint_log_probability_f32_bits: u32_bits_hex(term.joint_log_probability_bits),
            value_f32_bits: u32_bits_hex(term.value_bits),
            terminal_return: term.terminal_return,
        });
    }
    Ok(BuiltTermStream {
        records,
        sha256: hex_bytes(&digest.finalize()),
        policy_nonzero_count,
        value_nonzero_count,
        terminal_return_counts: return_counts,
    })
}

fn selected_stream_sha256(
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<String, AppError> {
    let mut digest = Sha256::new();
    for selected in &observation.selected_outputs {
        digest.update(
            u64::try_from(selected.group_index)
                .map_err(|_| AppError::new("selected group index does not fit u64"))?
                .to_be_bytes(),
        );
        digest.update(
            u64::try_from(selected.substep_index)
                .map_err(|_| AppError::new("selected substep index does not fit u64"))?
                .to_be_bytes(),
        );
        digest.update(
            u64::try_from(selected.selected_action_index)
                .map_err(|_| AppError::new("selected action index does not fit u64"))?
                .to_be_bytes(),
        );
        digest.update(selected.selected_logit_bits.to_be_bytes());
        digest.update(selected.value_bits.to_be_bytes());
        digest.update(selected.selected_log_probability_bits.to_be_bytes());
    }
    Ok(hex_bytes(&digest.finalize()))
}

fn reconstruct_sequential_reduction(
    observation: &NativeTrainingUpdateObservationV2,
) -> Result<(f32, f32, f32), AppError> {
    let mut policy_sum = 0.0f32;
    let mut value_sum = 0.0f32;
    for term in &observation.physical_terms {
        let joint = f32::from_bits(term.joint_log_probability_bits);
        let value = f32::from_bits(term.value_bits);
        let target = f32::from(term.terminal_return);
        let advantage = target - value;
        let policy_term = -joint * advantage;
        let value_error = value - target;
        let value_term = value_error * value_error;
        policy_sum += policy_term;
        value_sum += value_term;
    }
    let group_count = observation.physical_terms.len() as f32;
    let loss = (policy_sum + VALUE_COEFFICIENT * value_sum) / group_count;
    if !policy_sum.is_finite() || !value_sum.is_finite() || !loss.is_finite() {
        return Err(AppError::new("reconstructed scalar is non-finite"));
    }
    Ok((policy_sum, value_sum, loss))
}

fn snapshot_record(snapshot: NativeTrainingSnapshotReceiptV1) -> SnapshotRecord {
    SnapshotRecord {
        schema: snapshot.schema,
        identity: snapshot.identity,
        snapshot_sha256: snapshot.snapshot_sha256,
        manifest_file_sha256: snapshot.manifest_file_sha256,
        manifest_core_sha256: snapshot.manifest_core_sha256,
        payload_sha256: snapshot.payload_sha256,
        payload_byte_count: snapshot.payload_byte_count,
        parameter_layout_sha256: snapshot.parameter_layout_sha256,
        named_parameter_stream_sha256: snapshot.named_parameter_stream_sha256,
        loaded_named_parameter_stream_sha256: snapshot.loaded_named_parameter_stream_sha256,
        model_config_fingerprint: snapshot.model_config_fingerprint,
        model_architecture_version: snapshot.model_architecture_version,
        feature_contract_digest: snapshot.feature_contract_digest,
        feature_encoding_digest: snapshot.feature_encoding_digest,
        initializer_identity: snapshot.initializer_identity,
        base_seed: snapshot.base_seed,
        model_init_seed: snapshot.model_init_seed,
        trainer_schedule_version: snapshot.trainer_schedule_version,
        python_reference_seed_version: snapshot.python_reference_seed_version,
        schedule_goldens_sha256: snapshot.schedule_goldens_sha256,
        authority_source_bundle_sha256: snapshot.authority_source_bundle_sha256,
        authority_runtime_identity: snapshot.authority_runtime_identity,
        loader_identity: snapshot.loader_identity,
        optimizer_identity: snapshot.optimizer_identity,
        adam_step_initial: snapshot.adam_step_initial,
        scorer_bias_anchor_f32_bits: snapshot.scorer_bias_anchor_f32_bits,
    }
}

fn seat_code(seat: PlayerSeatV1) -> u8 {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn seat_name(seat: PlayerSeatV1) -> &'static str {
    match seat {
        PlayerSeatV1::P0 => "p0",
        PlayerSeatV1::P1 => "p1",
    }
}

fn terminal_name(outcome: TerminalOutcomeV1) -> &'static str {
    match outcome {
        TerminalOutcomeV1::P0Win => "p0_win",
        TerminalOutcomeV1::P1Win => "p1_win",
        TerminalOutcomeV1::Draw => "draw",
        TerminalOutcomeV1::Truncated => "truncated",
        TerminalOutcomeV1::Halted => "halted",
    }
}

fn scalar_record(value: f32) -> ScalarRecord {
    ScalarRecord {
        value: f64::from(value),
        f32_bits: f32_bits_hex(value),
    }
}

fn f32_bits_hex(value: f32) -> String {
    u32_bits_hex(value.to_bits())
}

fn u32_bits_hex(bits: u32) -> String {
    format!("0x{bits:08x}")
}

fn sha256_file(path: &Path) -> Result<String, AppError> {
    let bytes = fs::read(path).map_err(|_| AppError::new("file hash read failed"))?;
    Ok(sha256_bytes(&bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_args() -> Vec<OsString> {
        [
            "--workers",
            "4",
            "--sessions-per-worker",
            "4",
            "--broker-target",
            "16",
            "--output",
            "E:/capture.json",
            "--expected-source-commit",
            "0123456789abcdef0123456789abcdef01234567",
            "--expected-source-tree",
            "89abcdef0123456789abcdef0123456789abcdef0123456789abcdef01234567",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    #[test]
    fn parser_retains_topology_revision_and_safe_snapshot_defaults() {
        let ParseOutcome::Run(parsed) = parse_args(valid_args()).unwrap() else {
            panic!("expected runnable arguments");
        };
        assert_eq!(parsed.workers, 4);
        assert_eq!(parsed.sessions_per_worker, 4);
        assert_eq!(parsed.broker_target, 16);
        assert_eq!(parsed.output, PathBuf::from("E:/capture.json"));
        assert_eq!(parsed.expected_source_tree.len(), 64);
        assert_eq!(
            (parsed.snapshot_manifest, parsed.snapshot_payload),
            repository_snapshot_paths().unwrap()
        );
    }

    #[test]
    fn parser_rejects_unsafe_or_ambiguous_inputs() {
        let mut oversized = valid_args();
        oversized[5] = OsString::from("17");
        assert!(parse_args(oversized)
            .unwrap_err()
            .to_string()
            .contains("must not exceed"));

        let mut uppercase_oid = valid_args();
        uppercase_oid[9] = OsString::from("0123456789ABCDEF0123456789abcdef01234567");
        assert!(parse_args(uppercase_oid)
            .unwrap_err()
            .to_string()
            .contains("lowercase hex"));

        let mut unpaired_snapshot = valid_args();
        unpaired_snapshot.extend([
            OsString::from("--snapshot-manifest"),
            OsString::from("manifest.json"),
        ]);
        assert!(parse_args(unpaired_snapshot)
            .unwrap_err()
            .to_string()
            .contains("must be supplied together"));

        assert_eq!(
            parse_args([OsString::from("--help")]).unwrap(),
            ParseOutcome::Help
        );
    }

    #[test]
    fn stream_hash_helpers_are_stable() {
        assert!(!USAGE.contains("\n+"));
        assert!(USAGE.contains("<64-lower-hex>"));
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(f32_bits_hex(0.5), "0x3f000000");
        assert_eq!(f32_bits_hex(0.001), "0x3a83126f");
    }
}
