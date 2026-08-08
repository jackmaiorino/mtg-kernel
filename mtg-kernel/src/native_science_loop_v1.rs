//! One-command native science loop: trainer, runner, and evaluator over one
//! durable Store.
//!
//! This library entry point bootstraps or reopens a Store, trains to the
//! run's exact target through the resume orchestration (every window on a
//! reconstructed executor), fully validates the Store, loads the update-zero
//! and latest trained boundaries through the complete decode chain, runs the
//! checkpoint-backed runner for both, and evaluates the seat-swapped uniform
//! reward delta. It is a development workflow product: it publishes no
//! experiment manifest and claims no experiment authority; the authoritative
//! one-command experiment product remains gated on the joint Store/CLI
//! freeze. On non-Windows platforms the loop fails with the stable
//! unsupported-platform classification before any path-backed mutation.

use crate::native_checkpoint_evaluator_v1::{
    evaluate_native_checkpoint_uniform_delta_v1, NativeCheckpointUniformDeltaEvaluationV1,
};
use crate::native_checkpoint_runner_v1::{
    run_native_checkpoint_v1, run_native_checkpoint_wide_v1, NativeCheckpointRunResultV1,
    NativeCheckpointRunnerConfigV1,
};
use crate::native_ladder_opponent_v1::LadderOpponentEngineV1;
#[cfg(test)]
use crate::native_policy_value_net_v1::{NativePolicyValueModelConfigV1, NativePolicyValueNetV1};
use crate::native_population_opponent_v1::{
    PopulationOpponentEngineV1, PopulationWeightVectorV1,
};
#[cfg(test)]
use crate::native_train_state_payload_v1::decode_native_train_state_payload_v1;
#[cfg(test)]
use crate::native_trainer_v1::{NativePolicyAnchorCoefficientV1, NativePolicyAnchorRuntimeV1};
use crate::native_training_executor_v1::{
    NativeTrainingExecutionConfigV1, NativeTrainingExecutorV1,
};
use crate::native_training_store_bootstrap_v2::{
    bootstrap_native_training_store_v2, NativeTrainingStoreBootstrapV2ErrorKind,
};
use crate::native_training_store_boundary_v2::build_genesis_native_training_boundary_v2;
use crate::native_training_store_checkpoint_v3::{
    build_genesis_checkpoint_manifest_v2_v3, build_genesis_checkpoint_manifest_v3,
    derive_genesis_weights_only_payload_v2_v3, GenesisInitializationReferenceV2,
};
use crate::native_training_store_prepared_segment_v2::prepare_segment_v2;
use crate::native_training_store_reference_latest_v2::{
    build_checkpoint_reference_v2, build_latest_v2,
};
use crate::native_training_store_resume_v2::{
    load_native_training_boundary_v2, resume_native_training_store_v2,
    validate_native_training_store_v2, NativeTrainingStoreResumeV2,
};
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::ValidatedTrainRunV2;
use crate::native_training_store_segment_manifest_v2::build_genesis_segment_manifest_v2;
use crate::native_training_store_update_group_v1::validate_prepared_execution_config_v1;
use crate::native_training_store_v2::{
    publish_genesis_generation_v2, NativeTrainingStorePublisherV2ErrorKind,
};
use std::error::Error;
#[cfg(test)]
use std::ffi::OsStr;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeScienceLoopV1ErrorKind {
    UnsupportedPlatform,
    StoreBusy,
    InputInvalid,
    BootstrapFailed,
    GenesisFailed,
    TrainFailed,
    ValidateFailed,
    LoadFailed,
    RunFailed,
    EvaluateFailed,
}

impl NativeScienceLoopV1ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "native-training-store-v2-unsupported-platform",
            Self::StoreBusy => "native-training-store-busy",
            Self::InputInvalid => "native-science-loop-input-invalid",
            Self::BootstrapFailed => "native-science-loop-bootstrap-failed",
            Self::GenesisFailed => "native-science-loop-genesis-failed",
            Self::TrainFailed => "native-science-loop-train-failed",
            Self::ValidateFailed => "native-science-loop-validate-failed",
            Self::LoadFailed => "native-science-loop-load-failed",
            Self::RunFailed => "native-science-loop-run-failed",
            Self::EvaluateFailed => "native-science-loop-evaluate-failed",
        }
    }
}

/// Redacted science-loop error carrying only its phase classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeScienceLoopV1Error {
    kind: NativeScienceLoopV1ErrorKind,
}

impl NativeScienceLoopV1Error {
    pub const fn kind(self) -> NativeScienceLoopV1ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeScienceLoopV1Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeScienceLoopV1Error {}

type Result<T> = std::result::Result<T, NativeScienceLoopV1Error>;

const fn loop_error_v1(kind: NativeScienceLoopV1ErrorKind) -> NativeScienceLoopV1Error {
    NativeScienceLoopV1Error { kind }
}

#[cfg(test)]
fn parse_policy_anchor_coefficient_v1(
    raw: Option<&OsStr>,
) -> std::result::Result<Option<NativePolicyAnchorCoefficientV1>, ()> {
    match raw.and_then(OsStr::to_str) {
        None => Ok(None),
        Some("0") => Ok(None),
        Some("0.01") => Ok(Some(NativePolicyAnchorCoefficientV1::Beta0p01)),
        Some("0.03") => Ok(Some(NativePolicyAnchorCoefficientV1::Beta0p03)),
        Some("0.1") => Ok(Some(NativePolicyAnchorCoefficientV1::Beta0p1)),
        Some("0.3") => Ok(Some(NativePolicyAnchorCoefficientV1::Beta0p3)),
        Some(_) => Err(()),
    }
}

#[cfg(test)]
fn parse_optional_generation_value_v1(raw: Option<&OsStr>) -> std::result::Result<Option<u64>, ()> {
    match raw {
        None => Ok(None),
        Some(raw) => raw
            .to_str()
            .ok_or(())?
            .parse::<u64>()
            .map(Some)
            .map_err(|_| ()),
    }
}

#[cfg(test)]
fn parse_optional_generation_knob_v1(name: &str) -> std::result::Result<Option<u64>, ()> {
    let raw = std::env::var_os(name);
    parse_optional_generation_value_v1(raw.as_deref())
}

#[cfg(test)]
fn policy_anchor_runtime_v1(
    ladder_init_reference: Option<&GenesisInitializationReferenceV2>,
) -> std::result::Result<Option<NativePolicyAnchorRuntimeV1>, ()> {
    let raw = std::env::var_os("MULTIRUN_POLICY_ANCHOR_BETA");
    let Some(coefficient) = parse_policy_anchor_coefficient_v1(raw.as_deref())? else {
        return Ok(None);
    };
    let reference = ladder_init_reference.ok_or(())?;
    let train_state = reference.checkpoint.train_state();
    let scorer_bias_anchor_bits =
        u32::try_from(train_state.scorer_bias_anchor_f32_bits()).map_err(|_| ())?;
    let decoded = decode_native_train_state_payload_v1(
        &reference.payload,
        train_state.adam_step(),
        scorer_bias_anchor_bits,
    )
    .map_err(|_| ())?;
    if decoded.digests.model_parameter_sha256 != reference.checkpoint.model_parameter_sha256() {
        return Err(());
    }
    let mut model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .map_err(|_| ())?;
    model
        .replace_parameter_snapshot_v1(&decoded.snapshot.parameters)
        .map_err(|_| ())?;
    if model.parameter_manifest_sha256_v1() != train_state.model_parameter_sha256() {
        return Err(());
    }
    Ok(Some(NativePolicyAnchorRuntimeV1 {
        coefficient,
        model: Arc::new(model),
    }))
}

#[cfg(test)]
fn response_exploiter_runtime_bindings_match_v1(
    declared_beta_f32_bits: &str,
    declared_weight_units: &[u64; 8],
    declared_weight_total_units: u64,
    installed_anchor: Option<NativePolicyAnchorCoefficientV1>,
    runtime_weights: Option<&PopulationWeightVectorV1>,
) -> bool {
    let Ok(declared_beta_bits) = u32::from_str_radix(declared_beta_f32_bits, 16) else {
        return false;
    };
    let Some(runtime_weights) = runtime_weights else {
        return false;
    };
    // De-novo screen declares beta bits 0x00000000 (0.0f32) and installs no
    // anchor at all -- `policy_anchor_runtime_v1` returns `Ok(None)` for the
    // literal "0" before ever touching a ladder-init reference. Every other
    // declared beta (build/screen's 0.1/0.03) still requires a real
    // installed anchor whose bits match exactly, unchanged from before this
    // arm was added.
    let anchor_matches = if declared_beta_bits == 0 {
        installed_anchor.is_none()
    } else {
        installed_anchor.is_some_and(|anchor| declared_beta_bits == anchor.bits_v1())
    };
    anchor_matches
        && declared_weight_units == runtime_weights.weights_v1()
        && declared_weight_total_units == runtime_weights.total_v1()
}

/// De-novo response screen (CLAUDE-DENOVO-SCREEN-SHEET-V1.md): the pure
/// boolean predicate behind the multirun harness's response-exploiter
/// runtime assert, extracted so both branches (warm-start "build"/"screen"
/// and fresh-init "denovo-screen") are unit-testable without spawning a real
/// training run. `ladder_init_present` and `denovo_enabled` are mutually
/// exclusive by construction: a warm-start response-exploiter run always
/// supplies `MULTIRUN_LADDER_INIT_STORE`; a denovo-screen run never does and
/// instead sets `MULTIRUN_RESPONSE_EXPLOITER_DENOVO=1`. Any other
/// combination (both, or neither) is invalid and this returns `false`.
///
/// Phase 2 horizon amendment (CLAUDE-DENOVO-SCREEN-SHEET-V1.md): 512 updates
/// is permitted, but only alongside `denovo_enabled` (the "denovo-screen-512"
/// role) -- warm-started "build"/"screen" runs stay fixed at 256, unchanged.
/// The record-level contract (`validate_response_exploiter_v1`,
/// native_training_store_run_v2) is the authority that further ties the
/// exact seed to the exact update count; this predicate only enforces the
/// coarser structural shape.
#[cfg(test)]
const fn response_exploiter_runtime_requirements_satisfied_v1(
    ladder_enabled: bool,
    environment_randomization_v2: bool,
    ladder_init_present: bool,
    denovo_enabled: bool,
    updates: u64,
    population_authority_enabled: bool,
) -> bool {
    ladder_enabled
        && environment_randomization_v2
        && (ladder_init_present != denovo_enabled)
        && (updates == 256 || (denovo_enabled && updates == 512))
        && !population_authority_enabled
}

#[cfg(test)]
fn validate_response_exploiter_runtime_bindings_v1(
    run: &ValidatedTrainRunV2,
    installed_anchor: Option<NativePolicyAnchorCoefficientV1>,
    runtime_weights: Option<&PopulationWeightVectorV1>,
) -> Result<()> {
    let Some(response) = run.record().contracts().response_exploiter_v1.as_ref() else {
        return Ok(());
    };
    if !response_exploiter_runtime_bindings_match_v1(
        &response.policy_anchor_beta_f32_bits,
        &response.effective_weight_units,
        response.effective_weight_total_units,
        installed_anchor,
        runtime_weights,
    ) {
        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid));
    }
    Ok(())
}

fn map_busy_v1<K>(
    kind: NativeScienceLoopV1ErrorKind,
    busy: impl Fn(&K) -> bool,
    unsupported: impl Fn(&K) -> bool,
) -> impl Fn(K) -> NativeScienceLoopV1Error {
    move |error| {
        loop_error_v1(if busy(&error) {
            NativeScienceLoopV1ErrorKind::StoreBusy
        } else if unsupported(&error) {
            NativeScienceLoopV1ErrorKind::UnsupportedPlatform
        } else {
            kind
        })
    }
}

const fn map_genesis_publisher_error_kind_v1(
    kind: NativeTrainingStorePublisherV2ErrorKind,
) -> NativeScienceLoopV1Error {
    loop_error_v1(match kind {
        NativeTrainingStorePublisherV2ErrorKind::UnsupportedPlatform => {
            NativeScienceLoopV1ErrorKind::UnsupportedPlatform
        }
        NativeTrainingStorePublisherV2ErrorKind::StoreBusy => {
            NativeScienceLoopV1ErrorKind::StoreBusy
        }
        _ => NativeScienceLoopV1ErrorKind::GenesisFailed,
    })
}

/// Move-only report of one complete science-loop invocation.
#[derive(Debug)]
pub struct NativeScienceLoopReportV1 {
    latest_generation_index: u64,
    reference_run: NativeCheckpointRunResultV1,
    candidate_run: NativeCheckpointRunResultV1,
    evaluation: NativeCheckpointUniformDeltaEvaluationV1,
}

impl NativeScienceLoopReportV1 {
    pub const fn latest_generation_index(&self) -> u64 {
        self.latest_generation_index
    }

    /// The update-zero reference run.
    pub const fn reference_run(&self) -> &NativeCheckpointRunResultV1 {
        &self.reference_run
    }

    /// The latest trained candidate run.
    pub const fn candidate_run(&self) -> &NativeCheckpointRunResultV1 {
        &self.candidate_run
    }

    /// The seat-swapped uniform reward-delta evaluation of candidate minus
    /// reference.
    pub const fn evaluation(&self) -> &NativeCheckpointUniformDeltaEvaluationV1 {
        &self.evaluation
    }
}

/// The ordinary science-loop genesis payload, dispatching on the record's
/// own wide claim and constructing through the crate-private run-bound
/// snapshot constructors in both branches, so the genesis executor's sealed
/// trajectory contract is the validated run's own decode-time
/// classification. Factored out of the loop so a callsite witness can drive
/// each branch directly: genesis bytes are mode-free, so only the run-bound
/// construction counters can prove which constructor ran.
fn ordinary_genesis_payload_run_bound_v2(
    run: &ValidatedTrainRunV2,
    execution_config: NativeTrainingExecutionConfigV1,
    snapshot_manifest_path: &Path,
    snapshot_payload_path: &Path,
) -> std::result::Result<Vec<u8>, ()> {
    let wide = run.record().contracts.wide_model_experiment_v1.is_some();
    let executor = if wide {
        NativeTrainingExecutorV1::from_common_model_snapshot_run_bound_wide_v2(
            execution_config,
            snapshot_manifest_path,
            snapshot_payload_path,
            run,
        )
        .map_err(|_| ())?
    } else {
        NativeTrainingExecutorV1::from_common_model_snapshot_run_bound_v2(
            execution_config,
            snapshot_manifest_path,
            snapshot_payload_path,
            run,
        )
        .map_err(|_| ())?
    };
    Ok(executor
        .checkpoint_candidate_v1()
        .map_err(|_| ())?
        .payload()
        .to_vec())
}

/// Run the complete one-command science loop.
///
/// Bootstrap or reopen the Store under `parent/root_basename`, publish the
/// genesis generation whenever no `latest.json` final exists, train to the
/// run's target entirely through resume-reconstructed executors, validate the
/// complete Store, then run and evaluate the update-zero and latest boundaries.
///
/// `ladder_opponent` is the Self-Play Ladder Design Contract S2 opponent
/// engine (Section 5): `None` reproduces today's uniform-opponent behavior
/// exactly and is what every caller outside the ladder pilot integration
/// passes; `Some(engine)` is threaded onto every window's reconstructed
/// executor before that window trains.
///
/// `LadderOpponentEngineV1` is deliberately `pub(crate)` (see its module
/// docs): only this crate can ever construct `Some(engine)`, so this stays a
/// sealed capability parameter for external callers, who can still call this
/// function with `None`.
///
/// `ladder_init_reference` is GenesisInitializationV2 (Self-Play Ladder
/// Design Contract S2, Amendment 1 / Section 8A point 2, Section 8B): the
/// caller's already-resolved continual-initialization reference checkpoint.
/// It MUST be `Some` if and only if `run`'s record carries
/// `contracts.opponent_ladder_initialization` -- any other combination fails
/// closed with [`NativeScienceLoopV1ErrorKind::GenesisFailed`] rather than
/// silently choosing a genesis source. `None` (the only value every caller
/// outside the continual-init ladder passes) reproduces today's
/// common-snapshot genesis behavior exactly, byte for byte. As of design
/// directive slice 2, publishing and every later resume/validate walk of a
/// ladder-init record no longer needs a resolved reference at all -- only
/// authoring generation 0 here does, because that is where the reference's
/// raw weights actually get copied into the candidate payload; see
/// `decode_genesis_checkpoint_manifest_dispatch_v2_v3` for the
/// self-contained decode the record's own `derived_model_parameter_sha256`
/// makes possible downstream of this call.
#[allow(private_interfaces, clippy::too_many_arguments)]
pub fn run_native_science_loop_v1(
    parent: impl AsRef<Path>,
    root_basename: &str,
    run: &ValidatedTrainRunV2,
    execution_config: NativeTrainingExecutionConfigV1,
    snapshot_manifest_path: &Path,
    snapshot_payload_path: &Path,
    runner_config: NativeCheckpointRunnerConfigV1,
    ladder_opponent: Option<Arc<LadderOpponentEngineV1>>,
    ladder_init_reference: Option<&GenesisInitializationReferenceV2>,
) -> Result<NativeScienceLoopReportV1> {
    match run_native_science_loop_with_opponents_v1(
        parent,
        root_basename,
        run,
        execution_config,
        snapshot_manifest_path,
        snapshot_payload_path,
        runner_config,
        ladder_opponent,
        None,
        ladder_init_reference,
        true,
    )? {
        NativeScienceLoopCompletionV1::Evaluated(report) => Ok(report),
        NativeScienceLoopCompletionV1::TrainingOnly { .. } => {
            Err(loop_error_v1(NativeScienceLoopV1ErrorKind::EvaluateFailed))
        }
    }
}

/// Experiment-facing population-runtime entry point. The existing science
/// loop and K4 entry point above remain unchanged and always install no
/// population engine.
#[allow(private_interfaces, clippy::too_many_arguments)]
pub fn run_native_science_loop_with_population_v1(
    parent: impl AsRef<Path>,
    root_basename: &str,
    run: &ValidatedTrainRunV2,
    execution_config: NativeTrainingExecutionConfigV1,
    snapshot_manifest_path: &Path,
    snapshot_payload_path: &Path,
    runner_config: NativeCheckpointRunnerConfigV1,
    population_opponent: Option<Arc<PopulationOpponentEngineV1>>,
    ladder_init_reference: Option<&GenesisInitializationReferenceV2>,
) -> Result<NativeScienceLoopReportV1> {
    if population_opponent.is_none()
        || run
            .record()
            .contracts()
            .population_program_v1
            .is_none()
    {
        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid));
    }
    match run_native_science_loop_with_opponents_v1(
        parent,
        root_basename,
        run,
        execution_config,
        snapshot_manifest_path,
        snapshot_payload_path,
        runner_config,
        None,
        population_opponent,
        ladder_init_reference,
        true,
    )? {
        NativeScienceLoopCompletionV1::Evaluated(report) => Ok(report),
        NativeScienceLoopCompletionV1::TrainingOnly { .. } => {
            Err(loop_error_v1(NativeScienceLoopV1ErrorKind::EvaluateFailed))
        }
    }
}

/// Response-exploiter training entry point. Unlike the general science loop,
/// this path stops after complete Store validation and never launches or reads
/// an automatic development evaluation. Qualification outcomes are produced
/// later by the separately frozen mixture and pure-anchor panels.
#[allow(private_interfaces, clippy::too_many_arguments)]
pub fn run_native_response_exploiter_training_v1(
    parent: impl AsRef<Path>,
    root_basename: &str,
    run: &ValidatedTrainRunV2,
    execution_config: NativeTrainingExecutionConfigV1,
    snapshot_manifest_path: &Path,
    snapshot_payload_path: &Path,
    runner_config: NativeCheckpointRunnerConfigV1,
    population_opponent: Option<Arc<PopulationOpponentEngineV1>>,
    ladder_init_reference: Option<&GenesisInitializationReferenceV2>,
) -> Result<u64> {
    if population_opponent.is_none()
        || run
            .record()
            .contracts()
            .response_exploiter_v1
            .is_none()
        || run
            .record()
            .contracts()
            .population_program_v1
            .is_some()
    {
        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid));
    }
    match run_native_science_loop_with_opponents_v1(
        parent,
        root_basename,
        run,
        execution_config,
        snapshot_manifest_path,
        snapshot_payload_path,
        runner_config,
        None,
        population_opponent,
        ladder_init_reference,
        false,
    )? {
        NativeScienceLoopCompletionV1::TrainingOnly {
            latest_generation_index,
        } => Ok(latest_generation_index),
        NativeScienceLoopCompletionV1::Evaluated(_) => {
            Err(loop_error_v1(NativeScienceLoopV1ErrorKind::EvaluateFailed))
        }
    }
}

enum NativeScienceLoopCompletionV1 {
    TrainingOnly { latest_generation_index: u64 },
    Evaluated(NativeScienceLoopReportV1),
}

#[allow(clippy::too_many_arguments)]
fn run_native_science_loop_with_opponents_v1(
    parent: impl AsRef<Path>,
    root_basename: &str,
    run: &ValidatedTrainRunV2,
    execution_config: NativeTrainingExecutionConfigV1,
    snapshot_manifest_path: &Path,
    snapshot_payload_path: &Path,
    runner_config: NativeCheckpointRunnerConfigV1,
    ladder_opponent: Option<Arc<LadderOpponentEngineV1>>,
    population_opponent: Option<Arc<PopulationOpponentEngineV1>>,
    ladder_init_reference: Option<&GenesisInitializationReferenceV2>,
    evaluate_after_training: bool,
) -> Result<NativeScienceLoopCompletionV1> {
    use crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2ErrorKind;

    if ladder_opponent.is_some() && population_opponent.is_some() {
        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid));
    }

    validate_prepared_execution_config_v1(run, &execution_config)
        .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid))?;

    #[cfg(test)]
    let policy_anchor = policy_anchor_runtime_v1(ladder_init_reference)
        .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid))?;
    #[cfg(test)]
    validate_response_exploiter_runtime_bindings_v1(
        run,
        policy_anchor.as_ref().map(|anchor| anchor.coefficient),
        population_opponent
            .as_deref()
            .map(PopulationOpponentEngineV1::weights_v1),
    )?;
    #[cfg(test)]
    let stop_after_generation = parse_optional_generation_knob_v1("MULTIRUN_STOP_AFTER_GENERATION")
        .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid))?;
    #[cfg(test)]
    let expected_resume_generation =
        parse_optional_generation_knob_v1("MULTIRUN_EXPECT_RESUME_GENERATION")
            .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid))?;

    // Bootstrap admits only the frozen B0 through B8 states.
    let bootstrapped = bootstrap_native_training_store_v2(parent.as_ref(), root_basename)
        .map_err(map_busy_v1(
        NativeScienceLoopV1ErrorKind::BootstrapFailed,
        |error: &crate::native_training_store_bootstrap_v2::NativeTrainingStoreBootstrapV2Error| {
            error.kind() == NativeTrainingStoreBootstrapV2ErrorKind::StoreBusy
        },
        |error| error.kind() == NativeTrainingStoreBootstrapV2ErrorKind::UnsupportedPlatform,
    ))?;
    // A missing latest final includes both a fresh skeleton and interrupted
    // bootstrap after exact run authority (and possibly candidate-equal
    // generation-zero finals or a latest stage). The publisher revalidates
    // the complete state under its own exclusive lock before any mutation.
    let genesis_required = !bootstrapped.latest_final_present();
    #[cfg(test)]
    if expected_resume_generation.is_some() && genesis_required {
        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid));
    }
    let root: ValidatedNativeTrainingStoreRootV2 = bootstrapped.into_root();

    // Train-new bootstrap and interrupted-bootstrap recovery both reconstruct
    // the exact genesis candidate from the independently attested common
    // snapshot. The receipt witnesses publication of exactly generation zero.
    if genesis_required {
        let genesis_error = loop_error_v1(NativeScienceLoopV1ErrorKind::GenesisFailed);
        // GenesisInitializationV2 branches strictly on the record's own
        // claim, never on caller convenience: a ladder-init record with no
        // supplied reference, or a supplied reference for a non-ladder-init
        // record, are both caller bugs and fail closed rather than silently
        // picking a source (Self-Play Ladder Design Contract S2, Amendment 1
        // / Section 8A point 2, Section 8B).
        let (checkpoint, payload) = match (
            run.record()
                .contracts
                .opponent_ladder_initialization
                .is_some(),
            ladder_init_reference,
        ) {
            (true, Some(reference)) => {
                let payload = derive_genesis_weights_only_payload_v2_v3(&reference.payload)
                    .map_err(|_| genesis_error)?;
                let checkpoint =
                    build_genesis_checkpoint_manifest_v2_v3(run, &reference.checkpoint, &payload)
                        .map_err(|_| genesis_error)?;
                (checkpoint, payload)
            }
            (false, None) => {
                // Capacity Experiment Contract (Stage 3), Section 3: genesis
                // authoring dispatches on the record's own
                // `contracts.wide_model_experiment_v1` claim, the same
                // record-driven signal `build_genesis_checkpoint_manifest_v3`
                // and the inference-authority chokepoint already dispatch
                // on -- this closes the wall's fail-closed-genesis finding,
                // the last of the four construction-dispatch sites the
                // contract's Section 3 enumerates. Absent, this reproduces
                // the frozen genesis path byte-for-byte.
                let payload = ordinary_genesis_payload_run_bound_v2(
                    run,
                    execution_config.clone(),
                    snapshot_manifest_path,
                    snapshot_payload_path,
                )
                .map_err(|_| genesis_error)?;
                let checkpoint = build_genesis_checkpoint_manifest_v3(run, &payload)
                    .map_err(|_| genesis_error)?;
                (checkpoint, payload)
            }
            (true, None) | (false, Some(_)) => return Err(genesis_error),
        };
        let segment =
            build_genesis_segment_manifest_v2(run, &checkpoint).map_err(|_| genesis_error)?;
        let boundary = build_genesis_native_training_boundary_v2(run, &segment, &checkpoint)
            .map_err(|_| genesis_error)?;
        let reference = build_checkpoint_reference_v2(run, &boundary).map_err(|_| genesis_error)?;
        let latest = build_latest_v2(&boundary, &reference).map_err(|_| genesis_error)?;
        let receipt = publish_genesis_generation_v2(
            &root,
            run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .map_err(|error| map_genesis_publisher_error_kind_v1(error.kind()))?;
        if receipt.generation_index() != 0 {
            return Err(genesis_error);
        }
    }

    // Train to the exact target: every window runs on a reconstructed
    // executor and commits only through the durable receipt.
    #[cfg(test)]
    let mut resume_generation_checked = expected_resume_generation.is_none();
    let latest_generation_index = loop {
        let resumed = resume_native_training_store_v2(&root, run, execution_config.clone())
            .map_err(map_busy_v1(
            NativeScienceLoopV1ErrorKind::TrainFailed,
            |error: &crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2Error| {
                error.kind() == NativeTrainingStoreResumeV2ErrorKind::StoreBusy
            },
            |error| error.kind() == NativeTrainingStoreResumeV2ErrorKind::UnsupportedPlatform,
        ))?;
        match resumed {
            NativeTrainingStoreResumeV2::Complete {
                latest_generation_index,
            } => {
                #[cfg(test)]
                if let Some(expected) = expected_resume_generation {
                    if !resume_generation_checked && latest_generation_index != expected {
                        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid));
                    }
                    resume_generation_checked = true;
                }
                break latest_generation_index;
            }
            NativeTrainingStoreResumeV2::Continue(mut continuation) => {
                let train_error = loop_error_v1(NativeScienceLoopV1ErrorKind::TrainFailed);
                #[cfg(test)]
                {
                    let parent_generation = continuation.parent_checkpoint.generation_index();
                    if let Some(expected) = expected_resume_generation {
                        if !resume_generation_checked {
                            if parent_generation != expected {
                                return Err(loop_error_v1(
                                    NativeScienceLoopV1ErrorKind::InputInvalid,
                                ));
                            }
                            resume_generation_checked = true;
                        }
                    }
                    if stop_after_generation == Some(parent_generation) {
                        break parent_generation;
                    }
                }
                // Self-Play Ladder Design Contract S2, Section 5. Every
                // window trains on a freshly reconstructed executor
                // (`resume_native_training_store_v2`'s own design); the
                // ladder engine (when configured) is threaded onto each one
                // here, before that window's update runs.
                continuation
                    .executor
                    .set_ladder_opponent_v1(ladder_opponent.clone());
                continuation
                    .executor
                    .set_population_opponent_v1(population_opponent.clone());
                #[cfg(test)]
                continuation
                    .executor
                    .set_policy_anchor_v1(policy_anchor.clone())
                    .map_err(|error| {
                        eprintln!(
                            "science-loop policy-anchor install failure: kind={:?} code={}",
                            error.kind(),
                            error.code()
                        );
                        train_error
                    })?;
                let prepared = prepare_segment_v2(
                    &mut continuation.executor,
                    run,
                    &continuation.parent_boundary,
                    &continuation.parent_checkpoint,
                )
                .map_err(|error| {
                    #[cfg(test)]
                    eprintln!(
                        "science-loop segment preparation failure: kind={:?} code={}",
                        error.kind(),
                        error.code()
                    );
                    #[cfg(not(test))]
                    let _ = error;
                    train_error
                })?;
                let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                    &root,
                    run,
                    &continuation.parent_boundary,
                    &continuation.parent_checkpoint,
                    &prepared,
                )
                .map_err(|error| {
                    #[cfg(test)]
                    eprintln!(
                        "science-loop segment publication failure: kind={:?} code={}",
                        error.kind(),
                        error.code()
                    );
                    #[cfg(not(test))]
                    let _ = error;
                    train_error
                })?;
                prepared.commit_v2(receipt).map_err(|error| {
                    #[cfg(test)]
                    eprintln!(
                        "science-loop segment commit failure: kind={:?} code={}",
                        error.kind(),
                        error.code()
                    );
                    #[cfg(not(test))]
                    let _ = error;
                    train_error
                })?;
            }
        }
    };
    #[cfg(test)]
    if !resume_generation_checked {
        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::InputInvalid));
    }

    // Full-store currentness validation after training.
    let state = validate_native_training_store_v2(&root, run)
        .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::ValidateFailed))?;
    if state.latest_generation_index() != latest_generation_index {
        return Err(loop_error_v1(NativeScienceLoopV1ErrorKind::ValidateFailed));
    }
    if !evaluate_after_training {
        return Ok(NativeScienceLoopCompletionV1::TrainingOnly {
            latest_generation_index,
        });
    }

    // Load the update-zero and latest boundaries through the complete decode
    // chain, then run both through the checkpoint-backed runner.
    let load_error = loop_error_v1(NativeScienceLoopV1ErrorKind::LoadFailed);
    let reference_boundary =
        load_native_training_boundary_v2(&root, run, 0).map_err(|_| load_error)?;
    let candidate_boundary = load_native_training_boundary_v2(&root, run, latest_generation_index)
        .map_err(|_| load_error)?;

    let run_error = loop_error_v1(NativeScienceLoopV1ErrorKind::RunFailed);
    // Capacity Experiment Contract (Stage 3), Section 3: the third and final
    // record-driven dispatch chokepoint this contract slice closes (genesis
    // authoring and checkpoint resume are the other two) -- both boundaries'
    // payloads carry the same architecture the record declares, so both eval
    // runs dispatch together on the same signal.
    let wide = run.record().contracts.wide_model_experiment_v1.is_some();
    let (reference_run, candidate_run) = if wide {
        let reference_run = run_native_checkpoint_wide_v1(
            run,
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
            runner_config,
        )
        .map_err(|_| run_error)?;
        let candidate_run = run_native_checkpoint_wide_v1(
            run,
            candidate_boundary.checkpoint(),
            candidate_boundary.payload(),
            runner_config,
        )
        .map_err(|_| run_error)?;
        (reference_run, candidate_run)
    } else {
        let reference_run = run_native_checkpoint_v1(
            run,
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
            runner_config,
        )
        .map_err(|_| run_error)?;
        let candidate_run = run_native_checkpoint_v1(
            run,
            candidate_boundary.checkpoint(),
            candidate_boundary.payload(),
            runner_config,
        )
        .map_err(|_| run_error)?;
        (reference_run, candidate_run)
    };

    let evaluation = evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run)
        .map_err(|_| loop_error_v1(NativeScienceLoopV1ErrorKind::EvaluateFailed))?;

    Ok(NativeScienceLoopCompletionV1::Evaluated(
        NativeScienceLoopReportV1 {
        latest_generation_index,
        reference_run,
        candidate_run,
        evaluation,
        },
    ))
}

#[cfg(test)]
mod policy_anchor_parse_tests {
    use super::*;

    #[test]
    fn policy_anchor_parser_accepts_only_frozen_exact_spellings() {
        assert_eq!(parse_policy_anchor_coefficient_v1(None), Ok(None));
        assert_eq!(
            parse_policy_anchor_coefficient_v1(Some(OsStr::new("0"))),
            Ok(None)
        );
        for (raw, expected) in [
            ("0.01", NativePolicyAnchorCoefficientV1::Beta0p01),
            ("0.03", NativePolicyAnchorCoefficientV1::Beta0p03),
            ("0.1", NativePolicyAnchorCoefficientV1::Beta0p1),
            ("0.3", NativePolicyAnchorCoefficientV1::Beta0p3),
        ] {
            assert_eq!(
                parse_policy_anchor_coefficient_v1(Some(OsStr::new(raw))),
                Ok(Some(expected))
            );
        }
        for raw in ["0.0", "-0", "1e-2", "0.10", "0.300000", "nan", "1"] {
            assert_eq!(
                parse_policy_anchor_coefficient_v1(Some(OsStr::new(raw))),
                Err(()),
                "unexpectedly accepted {raw}"
            );
        }
    }

    #[test]
    fn optional_generation_knob_parser_is_exact_u64() {
        assert_eq!(parse_optional_generation_value_v1(None), Ok(None));
        assert_eq!(
            parse_optional_generation_value_v1(Some(OsStr::new("32"))),
            Ok(Some(32))
        );
        assert_eq!(
            parse_optional_generation_value_v1(Some(OsStr::new("-1"))),
            Err(())
        );
    }

    #[test]
    fn response_exploiter_runtime_bindings_require_exact_installed_beta_and_weights() {
        let declared = [125_407, 115_542, 127_252, 127_098, 128_077, 127_916, 0, 0];
        let runtime = PopulationWeightVectorV1::new_v1(declared, 751_292).unwrap();
        assert!(response_exploiter_runtime_bindings_match_v1(
            "3dcccccd",
            &declared,
            751_292,
            Some(NativePolicyAnchorCoefficientV1::Beta0p1),
            Some(&runtime),
        ));
        assert!(!response_exploiter_runtime_bindings_match_v1(
            "3cf5c28f",
            &declared,
            751_292,
            Some(NativePolicyAnchorCoefficientV1::Beta0p1),
            Some(&runtime),
        ));
        let wrong_runtime = PopulationWeightVectorV1::new_v1(
            [125_408, 115_541, 127_252, 127_098, 128_077, 127_916, 0, 0],
            751_292,
        )
        .unwrap();
        assert!(!response_exploiter_runtime_bindings_match_v1(
            "3dcccccd",
            &declared,
            751_292,
            Some(NativePolicyAnchorCoefficientV1::Beta0p1),
            Some(&wrong_runtime),
        ));
        assert!(!response_exploiter_runtime_bindings_match_v1(
            "3dcccccd",
            &declared,
            751_291,
            Some(NativePolicyAnchorCoefficientV1::Beta0p1),
            Some(&runtime),
        ));
        assert!(!response_exploiter_runtime_bindings_match_v1(
            "3dcccccd",
            &declared,
            751_292,
            None,
            Some(&runtime),
        ));
    }

    #[test]
    fn response_exploiter_runtime_bindings_denovo_beta_requires_no_installed_anchor() {
        let declared = [125_407, 115_542, 127_252, 127_098, 128_077, 127_916, 0, 0];
        let runtime = PopulationWeightVectorV1::new_v1(declared, 751_292).unwrap();
        // Declared beta 0x00000000 (0.0f32) with no installed anchor: matches.
        assert!(response_exploiter_runtime_bindings_match_v1(
            "00000000",
            &declared,
            751_292,
            None,
            Some(&runtime),
        ));
        // Declared beta 0.0 but an anchor is actually installed: a real
        // mismatch between what the contract claims and what the trainer
        // built -- must reject, not silently accept.
        assert!(!response_exploiter_runtime_bindings_match_v1(
            "00000000",
            &declared,
            751_292,
            Some(NativePolicyAnchorCoefficientV1::Beta0p1),
            Some(&runtime),
        ));
        // Weight-vector binding still enforced under the denovo beta arm.
        let wrong_runtime = PopulationWeightVectorV1::new_v1(
            [125_408, 115_541, 127_252, 127_098, 128_077, 127_916, 0, 0],
            751_292,
        )
        .unwrap();
        assert!(!response_exploiter_runtime_bindings_match_v1(
            "00000000",
            &declared,
            751_292,
            None,
            Some(&wrong_runtime),
        ));
    }

    #[test]
    fn response_exploiter_runtime_requirements_accept_warm_start_xor_denovo_only() {
        // Warm-start ("build"/"screen"): ladder init present, denovo unset.
        assert!(response_exploiter_runtime_requirements_satisfied_v1(
            true, true, true, false, 256, false,
        ));
        // De-novo screen: ladder init absent, denovo set.
        assert!(response_exploiter_runtime_requirements_satisfied_v1(
            true, true, false, true, 256, false,
        ));
        // Neither warm-start nor denovo: invalid (a response-exploiter run
        // must declare exactly one genesis source).
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, true, false, false, 256, false,
        ));
        // Both warm-start and denovo: invalid (contradictory genesis claim).
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, true, true, true, 256, false,
        ));
        // Every other requirement stays load-bearing under the denovo arm
        // exactly as it already was under the warm-start arm.
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            false, true, false, true, 256, false,
        ));
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, false, false, true, 256, false,
        ));
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, true, false, true, 255, false,
        ));
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, true, false, true, 256, true,
        ));
        // Phase 2 horizon amendment (CLAUDE-DENOVO-SCREEN-SHEET-V1.md):
        // 512 updates is accepted, but only for the de-novo arm.
        assert!(response_exploiter_runtime_requirements_satisfied_v1(
            true, true, false, true, 512, false,
        ));
        // 512 updates is rejected for warm-start (build/screen never widen
        // past 256).
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, true, true, false, 512, false,
        ));
        // 512 updates is rejected without the de-novo arm enabled at all.
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, true, false, false, 512, false,
        ));
        // 512 updates still requires every other requirement (ladder,
        // envrand-v2, not population authority) to hold.
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            false, true, false, true, 512, false,
        ));
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, true, false, true, 512, true,
        ));
        // An update count that is neither 256 nor 512 is rejected even under
        // the de-novo arm.
        assert!(!response_exploiter_runtime_requirements_satisfied_v1(
            true, true, false, true, 511, false,
        ));
    }
}

#[cfg(all(test, windows))]
mod windows_science_loop_tests {
    use super::*;
    use crate::common_model_snapshot_v1::common_model_snapshot_paths_v1;
    use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
    use crate::native_training_store_bootstrap_v2::{
        bootstrap_native_training_store_v2, NativeTrainingStoreBootstrapOutcomeV2,
    };
    use crate::native_training_store_checkpoint_v3::decode_genesis_checkpoint_manifest_v2_v3;
    use crate::native_training_store_resume_v2::test_execution_config_v2;
    use crate::native_training_store_run_v2::{
        decode_train_run_v2, test_fixture_bytes_v2, test_fixture_bytes_with_schedule_v2,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::OnceLock;
    use std::time::Duration;

    struct TestParentV1 {
        parent: PathBuf,
    }

    impl TestParentV1 {
        fn new(label: &str) -> Self {
            static ORDINAL: AtomicU64 = AtomicU64::new(0);
            let ordinal = ORDINAL.fetch_add(1, Ordering::Relaxed);
            let parent = std::env::temp_dir().join(format!(
                "mtg-kernel-science-loop-v1-{}-{label}-{ordinal}",
                std::process::id()
            ));
            fs::create_dir(&parent).expect("create test parent");
            Self { parent }
        }
    }

    impl Drop for TestParentV1 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.parent);
        }
    }

    fn runner_config_v1() -> NativeCheckpointRunnerConfigV1 {
        NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: 2,
            scheduler_timeout: Duration::from_secs(300),
            measure_broker_service_time: false,
        }
    }

    fn one_segment_run_v1() -> ValidatedTrainRunV2 {
        decode_train_run_v2(&test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            2,
            4,
            4,
            2,
            4,
            8,
            32_768,
            65_536,
        ))
        .unwrap()
    }

    fn establish_exact_run_only_v1(parent: &TestParentV1, run: &ValidatedTrainRunV2) -> PathBuf {
        let bootstrapped = bootstrap_native_training_store_v2(&parent.parent, "store").unwrap();
        assert_eq!(
            bootstrapped.outcome(),
            NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        );
        assert!(!bootstrapped.latest_final_present());
        drop(bootstrapped);

        let root_path = parent.parent.join("store");
        fs::write(root_path.join("run.json"), run.canonical_bytes()).unwrap();
        root_path
    }

    fn assert_generation_directories_empty_v1(root_path: &Path) {
        for directory in ["segments", "checkpoints", "heads", "refs"] {
            assert_eq!(
                fs::read_dir(root_path.join(directory)).unwrap().count(),
                0,
                "{directory} must remain empty"
            );
        }
    }

    /// Learning smoke at the settled K=64 operating point: train a real
    /// episode count through the one-command loop on the CudaBurnDense
    /// backend, then report the seat-swapped uniform reward delta of the
    /// trained boundary against update-zero over enough evaluation pairs to
    /// see a direction. Diagnostic only: prints the delta and outcome
    /// counts, makes no learning-quality gate claim (the ratified gate has
    /// its own frozen estimands, seeds, and thresholds).
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn learning_smoke_k64_uniform_delta_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let updates = 128_u64;
        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            updates,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("smoke run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut execution_config = test_execution_config_v2(&run);
        execution_config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: 256,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };

        let parent = TestParentV1::new("learning-smoke");
        let started = std::time::Instant::now();
        let report = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            execution_config,
            &snapshot_manifest,
            &snapshot_payload,
            runner_config,
            None,
            None,
        )
        .expect("learning smoke loop");
        let wall = started.elapsed().as_secs_f64();

        let evaluation = report.evaluation();
        let reference = evaluation.reference_learner_outcomes();
        let candidate = evaluation.candidate_learner_outcomes();
        println!(
            "learning smoke: K=64 updates={updates} episodes={} wall={wall:.1}s",
            64 * updates
        );
        println!(
            "reference (update-zero) W/L/D: {}/{}/{} of {}",
            reference.wins(),
            reference.losses(),
            reference.draws(),
            reference.total()
        );
        println!(
            "candidate (gen {}) W/L/D: {}/{}/{} of {}",
            report.latest_generation_index(),
            candidate.wins(),
            candidate.losses(),
            candidate.draws(),
            candidate.total()
        );
        println!(
            "pairs={} total candidate-minus-reference reward delta = {}",
            evaluation.pair_count(),
            evaluation.total_candidate_minus_reference_reward_delta()
        );
    }

    /// Qualification measurement at 256-update depth: the diagnostic loop
    /// with the bridge's test-only measurement mode enabled, so the
    /// transported-logit gate records per-row f64 promotion metrics (delta
    /// range with worst-row identity and gauge-invariant row scale,
    /// selected-log-probability delta, per-decision cumulative joint error)
    /// instead of failing at the unratified bound. Measurement evidence for
    /// the gate qualification campaign; banks nothing.
    /// RAII guard for the bridge's measurement flag: arming sets it, and the
    /// drop runs on every exit path including panics, so no early failure
    /// can leak record-only mode into later same-process tests.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    struct MeasurementModeGuardV1;

    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    impl MeasurementModeGuardV1 {
        fn arm() -> Self {
            crate::experimental_burn_net8_packed_v1::bridge::TOLERANCE_MEASUREMENT_MODE_V1
                .store(true, std::sync::atomic::Ordering::Relaxed);
            Self
        }
    }

    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    impl Drop for MeasurementModeGuardV1 {
        fn drop(&mut self) {
            crate::experimental_burn_net8_packed_v1::bridge::TOLERANCE_MEASUREMENT_MODE_V1
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Multirun orchestration pilot: N concurrent science loops in one
    /// process, each with a distinct held-out base seed via the combined
    /// fixture helper, a distinct store root, and the full durable
    /// pipeline. Proves same-process isolation (per-root store locks,
    /// content-keyed resident device slot, per-run rollout state) and
    /// measures real aggregate throughput under contention. Non-banked
    /// diagnostic; wall-clock numbers are environment-dependent.
    ///
    /// Sweep knobs arrive via env so one binary serves every point:
    /// MULTIRUN_RUNS, MULTIRUN_UPDATES, MULTIRUN_WORKERS,
    /// MULTIRUN_SESSIONS, MULTIRUN_BROKER_TARGET,
    /// MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2, and
    /// MULTIRUN_POLICY_ANCHOR_BETA, MULTIRUN_STOP_AFTER_GENERATION, and
    /// MULTIRUN_EXPECT_RESUME_GENERATION. The broker target is
    /// the contention knob: the trainer sizes its forward pool as
    /// min(broker_batch_target, actors, cores), so N runs partition the
    /// machine's scoring cores only when N * target <= cores.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn multirun_pilot_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_and_base_seed_population_environment_v2,
            test_fixture_bytes_with_schedule_and_base_seed_v2,
        };

        fn env_knob_v1(name: &str, default: u64) -> u64 {
            match std::env::var(name) {
                Ok(raw) => raw.parse().expect("multirun knob must be a u64"),
                Err(_) => default,
            }
        }

        let run_count = usize::try_from(env_knob_v1("MULTIRUN_RUNS", 4)).unwrap();
        let updates = env_knob_v1("MULTIRUN_UPDATES", 16);
        let workers = env_knob_v1("MULTIRUN_WORKERS", 2);
        let sessions = env_knob_v1("MULTIRUN_SESSIONS", 32);
        let broker_target = env_knob_v1("MULTIRUN_BROKER_TARGET", 16);
        // S1 mirror-validation knobs (test-harness only; no model or
        // schedule semantics change): MULTIRUN_BASE_SEED replaces the
        // hardcoded 424_242, MULTIRUN_SEED_OFFSET disambiguates ordinals
        // across concurrent device processes so N seeds stay globally
        // distinct, and MULTIRUN_STORE_PARENT retains per-run stores under
        // a durable directory instead of the auto-deleting temp parent.
        let base_seed = env_knob_v1("MULTIRUN_BASE_SEED", 424_242);
        let seed_offset = env_knob_v1("MULTIRUN_SEED_OFFSET", 0);
        let durable_parent = std::env::var("MULTIRUN_STORE_PARENT").ok();
        // MULTIRUN_RECORD_ONLY=1 arms the bridge's record-only measurement
        // mode for the whole pilot, same guard as pathfinding_run_k64_deep_v1
        // (process-wide flag; the guard drops on every exit path). Without
        // it, depths past ~128 updates risk a mid-run fail-closed abort on
        // the transported-logit hard tolerance, exactly as measured in the
        // depth-drift characterization; the S1 mirror-validation plan
        // predeclares record-only for this reason.
        let record_only = env_knob_v1("MULTIRUN_RECORD_ONLY", 0) != 0;
        let _measurement_mode = if record_only {
            Some(MeasurementModeGuardV1::arm())
        } else {
            None
        };
        // Self-Play Ladder Design Contract S2, Section 6 (pilot). MULTIRUN_LADDER=1
        // builds a ladder opponent engine from MULTIRUN_LADDER_POOL_DIR (a
        // directory with three subdirectories -- primary/, pred-a/, pred-b/,
        // each a complete Store root for that pool member's source run; see
        // `native_ladder_pool_resolution_v1`'s module docs for why a full
        // Store root is required for a nonzero-generation checkpoint -- plus
        // pool.json carrying the `OpponentLadderPoolContractV1` JSON) and
        // threads the same engine into every spawned run. Each run's record
        // then carries the ladder identity + pool + schedule V2 sections
        // (`test_fixture_bytes_with_schedule_and_base_seed_ladder_v2`)
        // instead of the uniform identity; the uniform path (MULTIRUN_LADDER
        // unset) is completely untouched.
        let ladder_enabled = env_knob_v1("MULTIRUN_LADDER", 0) != 0;
        let population_authority_enabled =
            env_knob_v1("MULTIRUN_POPULATION_AUTHORITY", 0) != 0;
        let population_runtime_enabled = env_knob_v1("MULTIRUN_POPULATION_RUNTIME", 0) != 0;
        let response_exploiter_runtime_enabled =
            env_knob_v1("MULTIRUN_RESPONSE_EXPLOITER_RUNTIME", 0) != 0;
        // De-novo response screen (CLAUDE-DENOVO-SCREEN-SHEET-V1.md): an
        // explicit, independent boolean knob rather than an implicit
        // beta=="0" signal, matching this harness's existing one-concept-one-
        // knob style (MULTIRUN_WIDE, MULTIRUN_POPULATION_RUNTIME, ...).
        // Meaningful only alongside MULTIRUN_RESPONSE_EXPLOITER_RUNTIME=1;
        // see `response_exploiter_runtime_requirements_satisfied_v1`.
        let response_exploiter_denovo_enabled =
            env_knob_v1("MULTIRUN_RESPONSE_EXPLOITER_DENOVO", 0) != 0;
        // Macro Self-Play Envrand-V2 Rung V1. This knob changes only the
        // run-record trajectory declaration. Runtime dispatch remains owned
        // by the validated record and its sealed executor-mode diagonal.
        let environment_randomization_v2 =
            env_knob_v1("MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2", 0) != 0;
        let stop_after_generation =
            parse_optional_generation_knob_v1("MULTIRUN_STOP_AFTER_GENERATION")
                .expect("MULTIRUN_STOP_AFTER_GENERATION must be a u64");
        let expected_resume_generation =
            parse_optional_generation_knob_v1("MULTIRUN_EXPECT_RESUME_GENERATION")
                .expect("MULTIRUN_EXPECT_RESUME_GENERATION must be a u64");
        let ladder_pool: Option<crate::native_training_store_run_v2::OpponentLadderPoolContractV1> =
            if ladder_enabled {
                let pool_dir = std::env::var("MULTIRUN_LADDER_POOL_DIR")
                    .expect("MULTIRUN_LADDER=1 requires MULTIRUN_LADDER_POOL_DIR");
                let pool_dir = std::path::PathBuf::from(pool_dir);
                let pool_bytes = fs::read(pool_dir.join("pool.json"))
                    .expect("MULTIRUN_LADDER_POOL_DIR/pool.json");
                let pool = serde_json::from_slice(&pool_bytes)
                    .expect("pool.json must decode as OpponentLadderPoolContractV1");
                Some(pool)
            } else {
                None
            };
        let ladder_engine: Option<Arc<LadderOpponentEngineV1>> = match &ladder_pool {
            Some(pool) => {
                let pool_dir = std::path::PathBuf::from(
                    std::env::var("MULTIRUN_LADDER_POOL_DIR")
                        .expect("MULTIRUN_LADDER=1 requires MULTIRUN_LADDER_POOL_DIR"),
                );
                let (primary, predecessor_a, predecessor_b) =
                    crate::native_ladder_pool_resolution_v1::resolve_ladder_pool_v1(
                        pool,
                        &pool_dir.join("primary"),
                        &pool_dir.join("pred-a"),
                        &pool_dir.join("pred-b"),
                    )
                    .expect("ladder pool members must resolve to validated checkpoint handles");
                let engine = LadderOpponentEngineV1::new_v1(
                    pool.clone(),
                    primary,
                    predecessor_a,
                    predecessor_b,
                )
                .expect("MULTIRUN_LADDER_POOL_DIR/pool.json must match the frozen pool literals");
                Some(Arc::new(engine))
            }
            None => None,
        };
        // Self-Play Ladder Design Contract S2, Amendment 1 / Section 8A
        // point 2, Section 8B (GenesisInitializationV2). The pilot-harness
        // knobs the earlier attempt correctly withheld while the store
        // contract couldn't represent continual init: MULTIRUN_LADDER_INIT_STORE
        // (a complete Store root holding the reference checkpoint) and
        // MULTIRUN_LADDER_INIT_GEN (its generation, default 0) land now that
        // it can. Only meaningful with MULTIRUN_LADDER=1 (continual init is
        // a ladder-identity concept). The record's own
        // `opponent_ladder_initialization` section is populated entirely
        // from the referenced files themselves
        // (`stage_ladder_checkpoint_initialization_v1`, design directive
        // slice 2: the five-field digest-pin plus
        // `derived_model_parameter_sha256`, both computed from the SAME
        // resolved checkpoint), never hand-maintained; the reference
        // checkpoint is then resolved back from that record section through
        // the identical chain-proven loader (`ladder_init_as_checkpoint_ref_v1`
        // + `resolve_ladder_checkpoint_authority_v1`) a real, non-harness
        // caller would use. Resolved ONCE and shared across every spawned
        // run via `Arc`, mirroring `ladder_engine`, since
        // `CheckpointManifestV3` is deliberately not `Clone`.
        let ladder_init_store: Option<std::path::PathBuf> =
            std::env::var("MULTIRUN_LADDER_INIT_STORE")
                .ok()
                .map(std::path::PathBuf::from);
        let ladder_init_gen = env_knob_v1("MULTIRUN_LADDER_INIT_GEN", 0);
        let ladder_init_section: Option<
            crate::native_training_store_run_v2::OpponentLadderInitializationContractV1,
        > = ladder_init_store.as_ref().map(|store_dir| {
            assert!(
                ladder_enabled,
                "MULTIRUN_LADDER_INIT_STORE requires MULTIRUN_LADDER=1 (continual init is a ladder-identity concept)"
            );
            crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_initialization_v1(
                store_dir,
                ladder_init_gen,
            )
            .expect(
                "MULTIRUN_LADDER_INIT_STORE/MULTIRUN_LADDER_INIT_GEN must stage to a valid init section",
            )
        });
        let ladder_init_reference: Option<Arc<GenesisInitializationReferenceV2>> = match (
            &ladder_init_store,
            &ladder_init_section,
        ) {
            (Some(store_dir), Some(section)) => {
                let checkpoint_ref =
                    crate::native_ladder_pool_resolution_v1::ladder_init_as_checkpoint_ref_v1(
                        section,
                    );
                let authority = crate::native_ladder_pool_resolution_v1::resolve_ladder_checkpoint_authority_v1(
                        store_dir,
                        &checkpoint_ref,
                    )
                    .expect(
                        "MULTIRUN_LADDER_INIT_STORE checkpoint must resolve through the chain-proven loader",
                    );
                let (checkpoint, payload) = authority.into_checkpoint_and_payload();
                Some(Arc::new(GenesisInitializationReferenceV2 {
                    checkpoint,
                    payload,
                }))
            }
            _ => None,
        };
        assert!(
            !population_authority_enabled
                || (ladder_enabled
                    && environment_randomization_v2
                    && ladder_init_section.is_some()
                    && updates == 1_536),
            "MULTIRUN_POPULATION_AUTHORITY=1 requires the exact ladder, envrand-v2, parent-init, and global-1536 Run"
        );
        assert!(
            !population_runtime_enabled || population_authority_enabled,
            "MULTIRUN_POPULATION_RUNTIME=1 requires MULTIRUN_POPULATION_AUTHORITY=1"
        );
        assert!(
            !(population_runtime_enabled && response_exploiter_runtime_enabled),
            "population and response-exploiter runtimes are mutually exclusive"
        );
        assert!(
            !response_exploiter_denovo_enabled || response_exploiter_runtime_enabled,
            "MULTIRUN_RESPONSE_EXPLOITER_DENOVO=1 requires MULTIRUN_RESPONSE_EXPLOITER_RUNTIME=1"
        );
        assert!(
            !response_exploiter_runtime_enabled
                || response_exploiter_runtime_requirements_satisfied_v1(
                    ladder_enabled,
                    environment_randomization_v2,
                    ladder_init_section.is_some(),
                    response_exploiter_denovo_enabled,
                    updates,
                    population_authority_enabled,
                ),
            "response-exploiter runtime requires ladder pool, envrand-v2, exactly 256 updates \
             (or 512 updates, de-novo only -- Phase 2 horizon amendment), \
             and exactly one of parent init (warm-start build/screen) or \
             MULTIRUN_RESPONSE_EXPLOITER_DENOVO=1 (fresh-init denovo-screen)"
        );
        let population_engine = if population_runtime_enabled || response_exploiter_runtime_enabled {
            let (chain_name, roots_name) = if response_exploiter_runtime_enabled {
                (
                    "MULTIRUN_RESPONSE_EXPLOITER_REFRESH_CHAIN",
                    "MULTIRUN_RESPONSE_EXPLOITER_SLOT_ROOTS",
                )
            } else {
                (
                    "MULTIRUN_POPULATION_REFRESH_CHAIN",
                    "MULTIRUN_POPULATION_SLOT_ROOTS",
                )
            };
            let chain_paths: Vec<std::path::PathBuf> = std::env::var(chain_name)
            .unwrap_or_else(|_| panic!("runtime requires {chain_name}"))
            .split(';')
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .collect();
            assert!(!chain_paths.is_empty(), "population refresh chain is empty");
            let mut chain = Vec::with_capacity(chain_paths.len());
            for path in chain_paths {
                let bytes = fs::read(&path).expect("population refresh manifest must be readable");
                let manifest = crate::native_population_refresh_manifest_v1::decode_population_refresh_manifest_v1(
                    &bytes,
                    chain.last(),
                )
                .expect("population refresh chain must validate");
                chain.push(manifest);
            }
            let active = chain.last().expect("population refresh chain is nonempty");
            if population_runtime_enabled {
                let expected_start = expected_resume_generation
                    .expect("population runtime requires MULTIRUN_EXPECT_RESUME_GENERATION");
                let expected_stop = expected_start
                    .checked_add(128)
                    .expect("population interval generation overflow");
                assert_eq!(active.global_generation_v1(), expected_start);
                assert_eq!(stop_after_generation, Some(expected_stop));
            } else {
                assert_eq!(active.refresh_index_v1(), 8);
                assert_eq!(active.global_generation_v1(), 1_536);
                assert_eq!(
                    crate::native_training_store_digest_v1::lower_hex_raw32_v1(
                        active.manifest_sha256_v1()
                    ),
                    "9c9490b205b7b5a933eae7ca86916e5ff5ff9307a150dc35487a8e1c28e73e22",
                    "response-exploiter runtime refresh bytes differ from the RunV2 target authority"
                );
                assert_eq!(expected_resume_generation, None);
                assert!(
                    stop_after_generation.is_none() || stop_after_generation == Some(4),
                    "response-exploiter runtime permits only the four-update screen or full 256-update build"
                );
            }
            let slot_roots: Vec<std::path::PathBuf> = std::env::var(roots_name)
            .unwrap_or_else(|_| panic!("runtime requires {roots_name}"))
            .split(';')
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .collect();
            let engine = if response_exploiter_runtime_enabled {
                crate::native_population_runtime_resolution_v1::resolve_population_response_target_v1(
                    active,
                    &slot_roots,
                )
                .expect("response-exploiter target slots must resolve through Store authority")
            } else {
                crate::native_population_runtime_resolution_v1::resolve_population_opponent_v1(
                    active,
                    &slot_roots,
                )
                .expect("population runtime slots must resolve through Store authority")
            };
            Some(Arc::new(engine))
        } else {
            None
        };
        // Capacity Experiment Contract (Stage 3), Section 4 (task item 4).
        // MULTIRUN_WIDE=1 stamps every spawned run's record with the wide
        // (kernel-policy-value-net-8w128) architecture identity via the wide
        // (or wide+ladder-pool) fixture builder, and points genesis at the
        // wide production snapshot (data/wide_model_snapshot_w128/) instead
        // of the frozen common snapshot. Composes with MULTIRUN_LADDER=1
        // (the contract's own protocol: wide runs train against the ladder
        // pool2, "pinned BY CHECKPOINT REFERENCE") but NOT with
        // MULTIRUN_LADDER_INIT_STORE: continual init is a ladder-identity
        // concept the wide protocol deliberately never uses ("wide+ladder-init
        // is NOT needed - fresh init per contract"), so that combination
        // fails closed here rather than silently picking a genesis source.
        let wide_enabled = env_knob_v1("MULTIRUN_WIDE", 0) != 0;
        assert!(
            !(wide_enabled && ladder_init_store.is_some()),
            "MULTIRUN_WIDE=1 is incompatible with MULTIRUN_LADDER_INIT_STORE: \
             the wide protocol trains fresh-init only (contract Section 4)"
        );
        assert!(
            !(wide_enabled && environment_randomization_v2),
            "MULTIRUN_WIDE=1 is not part of the narrow envrand-v2 macro rung"
        );
        assert!(
            !(wide_enabled && population_authority_enabled),
            "population authority is fixed to the narrow Net8 runtime"
        );
        println!(
            "MULTIRUN CONFIG runs={run_count} updates={updates} topology={workers}x{sessions} \
             broker_target={broker_target} base_seed={base_seed} seed_offset={seed_offset} \
             record_only={record_only} ladder={ladder_enabled} \
             ladder_init={} wide={wide_enabled} envrand_v2={environment_randomization_v2} \
             population_authority={population_authority_enabled} \
             population_runtime={population_runtime_enabled} \
             response_exploiter_runtime={response_exploiter_runtime_enabled} \
             response_exploiter_denovo={response_exploiter_denovo_enabled} \
             policy_anchor_beta={} stop_after_generation={} expected_resume_generation={}",
            ladder_init_store.is_some(),
            std::env::var("MULTIRUN_POLICY_ANCHOR_BETA").unwrap_or_else(|_| "absent".to_owned()),
            stop_after_generation.map_or_else(|| "absent".to_owned(), |value| value.to_string()),
            expected_resume_generation
                .map_or_else(|| "absent".to_owned(), |value| value.to_string())
        );
        let started = std::time::Instant::now();
        let handles: Vec<_> = (0..run_count)
            .map(|ordinal| {
                let durable_parent = durable_parent.clone();
                let ladder_pool = ladder_pool.clone();
                let ladder_engine = ladder_engine.clone();
                let ladder_init_section = ladder_init_section.clone();
                let ladder_init_reference = ladder_init_reference.clone();
                let population_engine = population_engine.clone();
                std::thread::spawn(move || {
                    let run_seed = base_seed + seed_offset + ordinal as u64;
                    let patched = if response_exploiter_runtime_enabled
                        && response_exploiter_denovo_enabled
                    {
                        // De-novo screen: same ladder-pool/mixture binding,
                        // but no parent initialization at all -- the
                        // runtime assert above already proved
                        // `ladder_init_section` is `None` here.
                        crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_denovo_environment_v2(
                            NativeTrainingNumericalBackendV1::CudaBurnDense,
                            64,
                            4,
                            updates,
                            workers,
                            sessions,
                            broker_target,
                            1_024,
                            2_048,
                            run_seed,
                            ladder_pool
                                .as_ref()
                                .expect("denovo-screen Run requires ladder pool")
                                .clone(),
                        )
                    } else if response_exploiter_runtime_enabled {
                        crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_response_exploiter_environment_v2(
                            NativeTrainingNumericalBackendV1::CudaBurnDense,
                            64,
                            4,
                            updates,
                            workers,
                            sessions,
                            broker_target,
                            1_024,
                            2_048,
                            run_seed,
                            ladder_pool
                                .as_ref()
                                .expect("response-exploiter Run requires ladder pool")
                                .clone(),
                            ladder_init_section
                                .as_ref()
                                .expect("response-exploiter Run requires parent initialization")
                                .clone(),
                            match std::env::var("MULTIRUN_POLICY_ANCHOR_BETA").as_deref() {
                                Ok("0.1") => "3dcccccd",
                                Ok("0.03") => "3cf5c28f",
                                _ => panic!("response-exploiter beta must be exactly 0.1 or 0.03"),
                            },
                        )
                    } else if population_authority_enabled {
                        test_fixture_bytes_with_schedule_and_base_seed_population_environment_v2(
                            NativeTrainingNumericalBackendV1::CudaBurnDense,
                            64,
                            4,
                            updates,
                            workers,
                            sessions,
                            broker_target,
                            1_024,
                            2_048,
                            run_seed,
                            ladder_pool
                                .as_ref()
                                .expect("population Run requires ladder pool")
                                .clone(),
                            ladder_init_section
                                .as_ref()
                                .expect("population Run requires parent initialization")
                                .clone(),
                        )
                    } else if wide_enabled {
                        // wide_enabled && ladder_init_section.is_some() is
                        // already ruled out by the assert above this closure
                        // is built from; only the pool (or its absence)
                        // varies here.
                        match &ladder_pool {
                            Some(pool) => {
                                use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_wide_ladder_v2;
                                test_fixture_bytes_with_schedule_and_base_seed_wide_ladder_v2(
                                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                                    64,
                                    4,
                                    updates,
                                    workers,
                                    sessions,
                                    broker_target,
                                    1_024,
                                    2_048,
                                    run_seed,
                                    pool.clone(),
                                )
                            }
                            None => {
                                use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_wide_v2;
                                test_fixture_bytes_with_schedule_and_base_seed_wide_v2(
                                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                                    64,
                                    4,
                                    updates,
                                    workers,
                                    sessions,
                                    broker_target,
                                    1_024,
                                    2_048,
                                    run_seed,
                                )
                            }
                        }
                    } else {
                        match (
                            &ladder_pool,
                            &ladder_init_section,
                            environment_randomization_v2,
                        ) {
                            (Some(pool), Some(init), true) => {
                                crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_init_environment_v2(
                                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                                    64,
                                    4,
                                    updates,
                                    workers,
                                    sessions,
                                    broker_target,
                                    1_024,
                                    2_048,
                                    run_seed,
                                    pool.clone(),
                                    init.clone(),
                                )
                            }
                            (Some(pool), Some(init), false) => {
                                crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2(
                                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                                    64,
                                    4,
                                    updates,
                                    workers,
                                    sessions,
                                    broker_target,
                                    1_024,
                                    2_048,
                                    run_seed,
                                    pool.clone(),
                                    init.clone(),
                                )
                            }
                            (Some(pool), None, true) => {
                                crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_environment_v2(
                                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                                    64,
                                    4,
                                    updates,
                                    workers,
                                    sessions,
                                    broker_target,
                                    1_024,
                                    2_048,
                                    run_seed,
                                    pool.clone(),
                                )
                            }
                            (Some(pool), None, false) => {
                                use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_v2;
                                test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
                                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                                    64,
                                    4,
                                    updates,
                                    workers,
                                    sessions,
                                    broker_target,
                                    1_024,
                                    2_048,
                                    run_seed,
                                    pool.clone(),
                                )
                            }
                            (None, None, true) => {
                                crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_environment_v2(
                                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                                    64,
                                    4,
                                    updates,
                                    workers,
                                    sessions,
                                    broker_target,
                                    1_024,
                                    2_048,
                                    run_seed,
                                )
                            }
                            (None, None, false) => {
                                test_fixture_bytes_with_schedule_and_base_seed_v2(
                                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                                    64,
                                    4,
                                    updates,
                                    workers,
                                    sessions,
                                    broker_target,
                                    1_024,
                                    2_048,
                                    run_seed,
                                )
                            }
                            (None, Some(_), _) => panic!(
                                "MULTIRUN_LADDER_INIT_STORE requires MULTIRUN_LADDER=1 (continual init is a ladder-identity concept)"
                            ),
                        }
                    };
                    let run = decode_train_run_v2(&patched).expect("pilot run record");
                    if response_exploiter_runtime_enabled {
                        let response = run
                            .record()
                            .contracts()
                            .response_exploiter_v1
                            .as_ref()
                            .expect("response-exploiter Run requires its authority section");
                        let actual_completion = stop_after_generation.unwrap_or(updates);
                        assert_eq!(
                            response.expected_completion_generation, actual_completion,
                            "response-exploiter Run role must bind the external stop generation"
                        );
                        assert_eq!(
                            response.run_role,
                            if response_exploiter_denovo_enabled {
                                // Denovo-screen has no early-stop smoke
                                // variant in this phase: it is always the
                                // full-horizon run, matching "build" -- 256
                                // updates for the original "denovo-screen"
                                // role, or 512 for the Phase 2 horizon
                                // amendment's "denovo-screen-512" role
                                // (CLAUDE-DENOVO-SCREEN-SHEET-V1.md). The
                                // record's own contract (seed membership in
                                // RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_SEEDS_V1
                                // vs RESPONSE_EXPLOITER_AUTHORIZED_DENOVO_512_SEEDS_V1,
                                // native_training_store_run_v2) is the
                                // authority that ties the exact seed to the
                                // exact role; this assert only checks the
                                // coarser updates-to-role-name shape matches.
                                assert!(
                                    stop_after_generation.is_none(),
                                    "denovo-screen does not support an early stop generation"
                                );
                                if updates == 512 {
                                    "denovo-screen-512"
                                } else {
                                    "denovo-screen"
                                }
                            } else if stop_after_generation.is_some() {
                                "screen"
                            } else {
                                "build"
                            }
                        );
                    }
                    // Capacity Experiment Contract Section 3: genesis authors
                    // from the wide production snapshot instead of the frozen
                    // common snapshot whenever the record carries the wide
                    // section, mirroring every other record-driven dispatch
                    // chokepoint in this codebase.
                    let (snapshot_manifest, snapshot_payload) = if wide_enabled {
                        crate::common_model_snapshot_v1::wide_model_snapshot_paths_v1()
                    } else {
                        common_model_snapshot_paths_v1()
                    };
                    let mut execution_config = test_execution_config_v2(&run);
                    execution_config.numerical_backend =
                        NativeTrainingNumericalBackendV1::CudaBurnDense;
                    let (parent_path, _ephemeral_guard) = match durable_parent {
                        Some(dir) => {
                            let path = std::path::PathBuf::from(dir)
                                .join(format!("run-{ordinal}"));
                            fs::create_dir_all(&path).expect("durable multirun parent");
                            (path, None)
                        }
                        None => {
                            let guard =
                                TestParentV1::new(&format!("multirun-pilot-{ordinal}"));
                            (guard.parent.clone(), Some(guard))
                        }
                    };
                    let wide_label = if wide_enabled {
                        format!(" label={}", crate::native_policy_value_net_v1::W_ARCHITECTURE_LABEL_V1)
                    } else {
                        String::new()
                    };
                    println!(
                        "MULTIRUN run={ordinal} seed={run_seed} store_root={}{wide_label}",
                        parent_path.join("store").display()
                    );
                    let runner_config = NativeCheckpointRunnerConfigV1 {
                        evaluation_base_seed: 7_777,
                        first_episode_index: 0,
                        episode_count: 32,
                        scheduler_timeout: Duration::from_secs(3_600),
                        measure_broker_service_time: false,
                    };
                    let run_started = std::time::Instant::now();
                    let (latest_generation_index, evaluation_counts) = if response_exploiter_runtime_enabled {
                        let generation = run_native_response_exploiter_training_v1(
                            &parent_path,
                            "store",
                            &run,
                            execution_config,
                            &snapshot_manifest,
                            &snapshot_payload,
                            runner_config,
                            population_engine.clone(),
                            ladder_init_reference.as_deref(),
                        )
                        .expect("response exploiter training loop");
                        (generation, None)
                    } else {
                        let report = if population_runtime_enabled {
                            run_native_science_loop_with_population_v1(
                                &parent_path,
                                "store",
                                &run,
                                execution_config,
                                &snapshot_manifest,
                                &snapshot_payload,
                                runner_config,
                                population_engine.clone(),
                                ladder_init_reference.as_deref(),
                            )
                        } else {
                            run_native_science_loop_v1(
                                &parent_path,
                                "store",
                                &run,
                                execution_config,
                                &snapshot_manifest,
                                &snapshot_payload,
                                runner_config,
                                ladder_engine.clone(),
                                ladder_init_reference.as_deref(),
                            )
                        }
                        .expect("pilot loop");
                        let outcomes = report.evaluation().candidate_learner_outcomes();
                        (
                            report.latest_generation_index(),
                            Some((outcomes.wins(), outcomes.losses())),
                        )
                    };
                    let wall = run_started.elapsed().as_secs_f64();
                    (
                        ordinal,
                        latest_generation_index,
                        wall,
                        evaluation_counts,
                    )
                })
            })
            .collect();
        let wide_label = if wide_enabled {
            format!(
                " label={}",
                crate::native_policy_value_net_v1::W_ARCHITECTURE_LABEL_V1
            )
        } else {
            String::new()
        };
        let mut total_episodes = 0_u64;
        for handle in handles {
            let (ordinal, generation, wall, evaluation_counts) =
                handle.join().expect("pilot thread");
            let expected_generation = stop_after_generation.unwrap_or(updates);
            assert_eq!(generation, expected_generation);
            total_episodes +=
                64 * generation.saturating_sub(expected_resume_generation.unwrap_or(0));
            match evaluation_counts {
                Some((wins, losses)) => println!(
                    "MULTIRUN run={ordinal} gen={generation} wall={wall:.1}s eval W/L {wins}/{losses}{wide_label}"
                ),
                None => println!(
                    "MULTIRUN run={ordinal} gen={generation} wall={wall:.1}s training_only=true terminal_outcomes_read=false{wide_label}"
                ),
            }
        }
        if let Some(generation) = expected_resume_generation {
            println!("STORE CLOSE_REOPEN resume_generation={generation}");
        }
        let aggregate_wall = started.elapsed().as_secs_f64();
        println!(
            "MULTIRUN AGGREGATE runs={run_count} episodes={total_episodes} \
             wall={aggregate_wall:.1}s eps_per_s={:.2} (non-evidence){wide_label}",
            total_episodes as f64 / aggregate_wall
        );
    }

    /// Saturation-curve evaluation over a surviving pathfinding store:
    /// loads the boundary at each requested generation and runs the
    /// checkpoint runner against uniform opponents, printing win/loss/draw
    /// S1 mirror-validation checkpoint evaluation: identical body shape to
    /// pathfinding_saturation_eval_v1 but reconstructs the run record with
    /// the S1 training schedule (256 updates, workers=2, sessions=32,
    /// broker target 16) and the store's own base seed, since boundary
    /// loading validates the run identity the store was created under.
    /// Env: S1_STORE_ROOT, S1_EVAL_GENS (comma-separated), S1_BASE_SEED.
    /// 256 seat-swapped pairs per generation at evaluation_base_seed 7_777,
    /// the predeclared S1 estimator.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn s1_mirror_saturation_eval_v1() {
        use crate::native_checkpoint_runner_v1::run_native_checkpoint_wide_v1;
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_checkpoint_v3::CheckpointManifestV3;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_v2;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_wide_v2;

        let root_path = std::env::var("S1_STORE_ROOT")
            .expect("S1_STORE_ROOT must name the S1 per-seed store root");
        let generations: Vec<u64> = std::env::var("S1_EVAL_GENS")
            .expect("S1_EVAL_GENS must list generations")
            .split(',')
            .map(|token| token.trim().parse().expect("generation index"))
            .collect();
        let base_seed: u64 = std::env::var("S1_BASE_SEED")
            .expect("S1_BASE_SEED must give the store's base seed")
            .parse()
            .expect("base seed u64");
        let eval_pairs: u64 = std::env::var("S1_EVAL_PAIRS")
            .unwrap_or_else(|_| "256".to_owned())
            .parse()
            .expect("eval pair count");
        // Capacity-experiment wide-net knob (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md
        // Section 3): WIDE=1 reconstructs the run record via the wide fixture
        // builder (so `load_native_training_boundary_v2` accepts a wide
        // store's boundaries: boundary loading validates the run identity
        // the store was created under) and evaluates through
        // `run_native_checkpoint_wide_v1` instead. Unset/0 reproduces this
        // probe's frozen behavior byte-for-byte.
        let wide = std::env::var("WIDE").is_ok_and(|value| value != "0");

        let patched = if wide {
            test_fixture_bytes_with_schedule_and_base_seed_wide_v2(
                NativeTrainingNumericalBackendV1::CudaBurnDense,
                64,
                4,
                256,
                2,
                32,
                16,
                1_024,
                2_048,
                base_seed,
            )
        } else {
            test_fixture_bytes_with_schedule_and_base_seed_v2(
                NativeTrainingNumericalBackendV1::CudaBurnDense,
                64,
                4,
                256,
                2,
                32,
                16,
                1_024,
                2_048,
                base_seed,
            )
        };
        let run = decode_train_run_v2(&patched).expect("s1 run record");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).unwrap();
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: eval_pairs,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };
        let run_checkpoint = |checkpoint: &CheckpointManifestV3, payload: &[u8]| {
            if wide {
                run_native_checkpoint_wide_v1(&run, checkpoint, payload, runner_config).unwrap()
            } else {
                run_native_checkpoint_v1(&run, checkpoint, payload, runner_config).unwrap()
            }
        };

        let reference_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let reference_run = run_checkpoint(
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
        );
        for generation in generations {
            let boundary = load_native_training_boundary_v2(&root, &run, generation).unwrap();
            let candidate_run = run_checkpoint(boundary.checkpoint(), boundary.payload());
            let evaluation =
                evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run)
                    .unwrap();
            let outcomes = evaluation.candidate_learner_outcomes();
            println!(
                "S1_SATURATION seed={base_seed} gen={generation} wide={wide} W/L/D {}/{}/{} of {} (delta vs gen0 {})",
                outcomes.wins(),
                outcomes.losses(),
                outcomes.draws(),
                outcomes.total(),
                evaluation.total_candidate_minus_reference_reward_delta()
            );
        }
    }

    /// Ladder-store checkpoint evaluation (Self-Play Ladder Design Contract
    /// S2 pilot, Deliverable 1): identical body shape to
    /// `s1_mirror_saturation_eval_v1`, but reconstructs the run record with
    /// the LADDER fixture builder (the SAME builder the pilot used to train
    /// the store, `test_fixture_bytes_with_schedule_and_base_seed_ladder_v2`
    /// -- commit 3ce8f48/ca58b58's fixture, with the SAME K=64/S=4 topology
    /// 2/32/16 the pilot's `pilot-rung1.sh` used) instead of the uniform
    /// fixture, so `load_native_training_boundary_v2` accepts the ladder
    /// store's boundaries (boundary loading validates the run identity the
    /// store was created under, and a ladder run's `run_sha256` differs from
    /// a uniform run's at the same schedule/topology/base-seed because the
    /// opponent contracts and pool section are hashed into it). The pool
    /// contract embedded in the reconstructed record is parsed byte-for-byte
    /// from `LADDER_POOL_JSON` (`OpponentLadderPoolContractV1`'s `Deserialize`
    /// impl, the SAME decode `multirun_pilot_v1`'s `MULTIRUN_LADDER_POOL_DIR`
    /// branch already uses), not hand-reconstructed, so the identity matches
    /// exactly.
    ///
    /// This is the panel/v0 (uniform-anchor) curve only (stopping policy
    /// 126fd81a..., Section 3's interim clause: "until [the policy-driven
    /// opponent] seat exists, an interim panel/v0 = uniform only applies");
    /// the frozen uniform opponent is the SAME opponent every uniform
    /// evaluation in this file already uses -- nothing about the OPPONENT
    /// side of this probe is ladder-specific, only the candidate/reference
    /// checkpoints' run-record identity is.
    ///
    /// The checkpoint runner (`run_native_checkpoint_v1`) needs no CUDA
    /// feature: it is the CPU dense inference path
    /// (`NativeCheckpointInferenceV1::score_decision_v1`), verified
    /// feature-gate-free in `native_checkpoint_runner_v1.rs` and
    /// `native_checkpoint_inference_v1.rs`; only the run record's DECLARED
    /// backend identity is `CudaBurnDense` (matching what the pilot actually
    /// trained under), which decodes and validates without the CUDA feature
    /// compiled in (see `cuda_backend_records_validate_and_mismatched_pairs_reject`,
    /// itself not feature-gated). This probe is therefore deliberately NOT
    /// gated behind `experimental-burn-net8-packed-cuda-v1`, unlike its
    /// sibling GPU-training probes in this file.
    ///
    /// Env: LADDER_STORE_ROOT, LADDER_EVAL_GENS (comma-separated),
    /// LADDER_BASE_SEED, LADDER_POOL_JSON (path to the pool.json whose
    /// contents the run record embeds). LADDER_EVAL_PAIRS (default 256,
    /// S1_EVAL_PAIRS-style knob, scoped to this probe only) sets
    /// `episode_count` directly, matching `s1_mirror_saturation_eval_v1`'s
    /// own `S1_EVAL_PAIRS` wiring convention byte-for-byte (that probe's
    /// knob is likewise a direct `episode_count` pass-through despite its
    /// name; mirrored here rather than "corrected" so this probe stays
    /// behaviorally identical to its S1 twin, as instructed). 256
    /// seat-swapped pairs at evaluation_base_seed 7_777 by default, the
    /// predeclared S1 estimator.
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn ladder_saturation_eval_v1() {
        use crate::native_checkpoint_runner_v1::run_native_checkpoint_wide_v1;
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_and_base_seed_ladder_v2,
            test_fixture_bytes_with_schedule_and_base_seed_wide_ladder_v2,
            OpponentLadderPoolContractV1,
        };

        let root_path = std::env::var("LADDER_STORE_ROOT")
            .expect("LADDER_STORE_ROOT must name the ladder per-seed store root");
        let generations: Vec<u64> = std::env::var("LADDER_EVAL_GENS")
            .expect("LADDER_EVAL_GENS must list generations")
            .split(',')
            .map(|token| token.trim().parse().expect("generation index"))
            .collect();
        let base_seed: u64 = std::env::var("LADDER_BASE_SEED")
            .expect("LADDER_BASE_SEED must give the store's base seed")
            .parse()
            .expect("base seed u64");
        let pool_json_path = std::env::var("LADDER_POOL_JSON")
            .expect("LADDER_POOL_JSON must name the pool.json path embedded in the run record");
        let eval_pairs: u64 = std::env::var("LADDER_EVAL_PAIRS")
            .unwrap_or_else(|_| "256".to_owned())
            .parse()
            .expect("eval pair count");
        // The reconstructed run identity must match the store's actual
        // schedule; the retry rungs train 512 updates (contract v4.1 8B).
        let ladder_updates: u64 = std::env::var("LADDER_UPDATES")
            .unwrap_or_else(|_| "256".to_owned())
            .parse()
            .expect("ladder updates");
        let init_store = std::env::var("LADDER_INIT_STORE").ok();
        let init_gen: u64 = std::env::var("LADDER_INIT_GEN")
            .unwrap_or_else(|_| "0".to_owned())
            .parse()
            .expect("init generation");

        let pool_bytes =
            fs::read(&pool_json_path).expect("LADDER_POOL_JSON must be a readable file");
        let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
            .expect("pool.json must decode as OpponentLadderPoolContractV1");
        // Capacity-experiment wide-net knob (CAPACITY-EXPERIMENT-CONTRACT-DRAFT.md
        // Section 3/4): WIDE=1 reconstructs the run record via the combined
        // wide+ladder fixture builder (the wide protocol trains against the
        // ladder pool, fresh-init only) and evaluates through
        // `run_native_checkpoint_wide_v1` instead. Unset/0 reproduces this
        // probe's frozen behavior byte-for-byte. Not combined with
        // LADDER_INIT_STORE: the wide protocol never uses continual init.
        let wide = std::env::var("WIDE").is_ok_and(|value| value != "0");
        assert!(
            !(wide && init_store.is_some()),
            "WIDE=1 is not supported with LADDER_INIT_STORE: the wide protocol trains fresh-init only"
        );

        let patched = match (&init_store, wide) {
            (Some(dir), false) => {
                let initialization =
                    crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_initialization_v1(
                        std::path::Path::new(dir),
                        init_gen,
                    )
                    .expect("stage eval init section");
                crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            ladder_updates,
            2,
            32,
            16,
            1_024,
            2_048,
            base_seed,
            pool,
                    initialization,
                )
            }
            (None, false) => test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
                NativeTrainingNumericalBackendV1::CudaBurnDense,
                64,
                4,
                ladder_updates,
                2,
                32,
                16,
                1_024,
                2_048,
                base_seed,
                pool,
            ),
            (None, true) => test_fixture_bytes_with_schedule_and_base_seed_wide_ladder_v2(
                NativeTrainingNumericalBackendV1::CudaBurnDense,
                64,
                4,
                ladder_updates,
                2,
                32,
                16,
                1_024,
                2_048,
                base_seed,
                pool,
            ),
            (Some(_), true) => unreachable!("guarded above"),
        };
        let run = decode_train_run_v2(&patched).expect("ladder run record");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).unwrap();
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: eval_pairs,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };
        let run_checkpoint =
            |checkpoint: &crate::native_training_store_checkpoint_v3::CheckpointManifestV3,
             payload: &[u8]| {
                if wide {
                    run_native_checkpoint_wide_v1(&run, checkpoint, payload, runner_config).unwrap()
                } else {
                    run_native_checkpoint_v1(&run, checkpoint, payload, runner_config).unwrap()
                }
            };

        let reference_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let reference_run = run_checkpoint(
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
        );
        for generation in generations {
            let boundary = load_native_training_boundary_v2(&root, &run, generation).unwrap();
            let candidate_run = run_checkpoint(boundary.checkpoint(), boundary.payload());
            let evaluation =
                evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run)
                    .unwrap();
            let outcomes = evaluation.candidate_learner_outcomes();
            println!(
                "LADDER_SATURATION seed={base_seed} gen={generation} wide={wide} W/L/D {}/{}/{} of {} (delta vs gen0 {})",
                outcomes.wins(),
                outcomes.losses(),
                outcomes.draws(),
                outcomes.total(),
                evaluation.total_candidate_minus_reference_reward_delta()
            );
        }
    }

    /// Head-to-head evaluator (Self-Play Ladder Design Contract S2 pilot,
    /// Deliverable 2): runs a candidate ladder checkpoint through the
    /// checkpoint runner with the LADDER ENGINE standing in the opponent
    /// seat, where the engine's three policy-driven pool slots are ALL
    /// independently loaded handles onto ONE fixed opponent checkpoint and
    /// the per-episode pool-choice is forced to always select a policy
    /// member (`LadderOpponentEngineV1::head_to_head_eval_v1`, EVAL-ONLY,
    /// `cfg(test)`-gated -- see its docs in `native_ladder_opponent_v1` for
    /// the full production-safety argument). Uses the EVAL-ONLY
    /// `run_native_checkpoint_with_ladder_opponent_eval_v1` runner twin (see
    /// `native_checkpoint_runner_v1` docs for why `run_native_checkpoint_v1`
    /// itself cannot take an engine without production-path surgery: its
    /// only opponent hook is a hardcoded `None`, and the executor-side
    /// ladder threading from da8e486 reaches only the training loop).
    ///
    /// Seat-swapped CRN pairing matches the S1 estimator convention
    /// automatically: the checkpoint runner's native schedule
    /// (`native_trainer_episode_schedule_v1`) already alternates the
    /// LEARNER's seat every consecutive episode while pairing episodes
    /// `2k`/`2k+1` onto the SAME environment seed, for ANY opponent
    /// (uniform or ladder-engine) -- this probe changes only who scores the
    /// opponent seat's decisions, not the schedule that decides seeding or
    /// seat alternation.
    ///
    /// The candidate side is reconstructed exactly like
    /// `ladder_saturation_eval_v1` (same fixture builder, same fixed
    /// K=64/S=4/topology-2-32-16 schedule, pool parsed from
    /// `H2H_CANDIDATE_POOL_JSON`). The opponent is loaded DIRECTLY from
    /// `H2H_OPPONENT_STORE_ROOT` (a full Store root: its own `run.json` plus
    /// the validated walk to that store's own latest generation) rather
    /// than through `native_ladder_pool_resolution_v1`'s ref-digest gate --
    /// permitted explicitly by the task ("digest optional here since this
    /// is eval-only"), and three independent
    /// `NativeCheckpointInferenceV1` handles are loaded from that ONE
    /// validated checkpoint (the type is deliberately not `Clone`) for the
    /// engine's three policy-driven slots.
    ///
    /// `H2H_PAIRS` (default 1_024) is the CRN PAIR count. This probe doubles
    /// `H2H_PAIRS` into `episode_count` so the run plays `2 * H2H_PAIRS`
    /// GAMES, matching the promotion gate's own literal (Amendment 1 /
    /// Section 8A point 1: `PROMOTION_GATE_GAME_COUNT_V1 = 2_048`, "the
    /// candidate plays 1,024 seat-swapped CRN pairs = 2,048 games... gate
    /// quantity = wins / 2,048"). This doubling also reproduces the
    /// contract's own predeclared "~8 minutes" head-to-head economics
    /// estimate (Section 4) far more closely than a non-doubled 1,024-game
    /// run would, which independently corroborates the doubling as the
    /// correct reading; the discrepancy with the panel-probe convention is
    /// disclosed rather than silently resolved.
    ///
    /// GATE FEED (Amendment 1 / Section 8A point 1): the gate's win-rate
    /// sub-check is fed the RAW GAME win count (`wins`, tabulated at leg
    /// granularity) over the raw game total, never the pair-level
    /// `pair_wins` this probe also tabulates below -- feeding pair wins into
    /// a game-denominated gate (or vice versa) silently misstates the win
    /// rate. `pair_wins` is retained ONLY as a labeled diagnostic (the
    /// contract's own "net-positive-per-pair" reading, REJECTED as the gate
    /// quantity: "at true parity under CRN seat symmetry, winning both legs
    /// of a pair is structurally rare, and that metric misrepresents parity
    /// as collapse").
    ///
    /// Env: H2H_CANDIDATE_STORE_ROOT, H2H_CANDIDATE_GEN,
    /// H2H_CANDIDATE_BASE_SEED, H2H_CANDIDATE_POOL_JSON,
    /// H2H_OPPONENT_STORE_ROOT, H2H_ENVIRONMENT_RANDOMIZATION_V2, H2H_PAIRS
    /// (default 1_024), H2H_EVAL_SEED (default 7_777). Prints
    /// `H2H candidate_gen=<g> W/L/D x/y/z of N` plus
    /// the promotion gate's win-rate sub-check verdict computed by actually
    /// calling `native_ladder_promotion_v1::promotion_gate_win_rate_passes_v1`
    /// on the tabulated raw game wins/games (Deliverable 4's "feed the
    /// results through the gate function").
    ///
    /// WIDE=1 (capacity contract Section 5 decisive read, added after
    /// hostile review caught this probe as the one unwired wide chokepoint):
    /// reconstructs the CANDIDATE record via the combined wide+ladder
    /// fixture builder and runs through
    /// `run_native_checkpoint_wide_with_ladder_opponent_eval_v1`. The
    /// OPPONENT side is deliberately untouched: every opponent in the
    /// capacity protocol (promoted(1), promoted(2), pool members) is a
    /// frozen Net8 checkpoint, and the frozen loader used for the engine's
    /// three handles fails closed on any wide-length payload, so a wide
    /// opponent can never be substituted silently. Same knob name and
    /// semantics as `ladder_saturation_eval_v1`; unset/0 reproduces the
    /// frozen probe byte-for-byte, and WIDE=1 with H2H_INIT_STORE is
    /// rejected (the wide protocol trains fresh-init only).
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn ladder_head_to_head_eval_v1() {
        use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
        use crate::native_checkpoint_runner_v1::{
            run_native_checkpoint_wide_with_ladder_opponent_eval_v1,
            run_native_checkpoint_with_ladder_opponent_eval_v1,
        };
        use crate::native_ladder_promotion_v1::promotion_gate_win_rate_passes_v1;
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_and_base_seed_ladder_v2,
            test_fixture_bytes_with_schedule_and_base_seed_wide_ladder_v2,
            OpponentLadderPoolContractV1,
        };
        use crate::rl::{
            terminal_tuple_is_valid_v1, PlayerSeatV1, TerminalClassificationV1, TerminalSafeCodeV2,
        };

        fn required_env_v1(name: &str) -> String {
            std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
        }

        let candidate_store_root = required_env_v1("H2H_CANDIDATE_STORE_ROOT");
        let candidate_gen: u64 = required_env_v1("H2H_CANDIDATE_GEN")
            .parse()
            .expect("candidate generation");
        let candidate_use_store_run =
            std::env::var("H2H_CANDIDATE_USE_STORE_RUN").is_ok_and(|value| value != "0");
        let candidate_base_seed: Option<u64> = if candidate_use_store_run {
            None
        } else {
            Some(
                required_env_v1("H2H_CANDIDATE_BASE_SEED")
                    .parse()
                    .expect("candidate base seed"),
            )
        };
        let candidate_pool_json = if candidate_use_store_run {
            None
        } else {
            Some(required_env_v1("H2H_CANDIDATE_POOL_JSON"))
        };
        let opponent_store_root = required_env_v1("H2H_OPPONENT_STORE_ROOT");
        let pairs: u64 = std::env::var("H2H_PAIRS")
            .unwrap_or_else(|_| "1024".to_owned())
            .parse()
            .expect("H2H_PAIRS");
        let eval_seed: u64 = std::env::var("H2H_EVAL_SEED")
            .unwrap_or_else(|_| "7777".to_owned())
            .parse()
            .expect("H2H_EVAL_SEED");
        let ladder_updates: u64 = std::env::var("H2H_UPDATES")
            .unwrap_or_else(|_| "256".to_owned())
            .parse()
            .expect("h2h updates");
        let init_store = std::env::var("H2H_INIT_STORE").ok();
        let init_gen: u64 = std::env::var("H2H_INIT_GEN")
            .unwrap_or_else(|_| "0".to_owned())
            .parse()
            .expect("init generation");
        let episode_count = pairs.checked_mul(2).expect("H2H_PAIRS overflow");
        // Capacity-experiment wide-net knob; see the doc comment. Candidate
        // side only; the opponent stays frozen-identity by protocol.
        let wide = std::env::var("WIDE").is_ok_and(|value| value != "0");
        let environment_randomization_v2 =
            std::env::var("H2H_ENVIRONMENT_RANDOMIZATION_V2").is_ok_and(|value| value != "0");
        assert!(
            !(wide && init_store.is_some()),
            "WIDE=1 is not supported with H2H_INIT_STORE: the wide protocol trains fresh-init only"
        );
        assert!(
            !(wide && environment_randomization_v2),
            "WIDE=1 is not part of the narrow envrand-v2 macro rung"
        );

        // Existing ladder probes reconstruct the candidate Run from their
        // frozen knobs. Population checkpoints instead opt in to loading the
        // exact Run already authenticated by their Store root. The old path
        // and bytes remain unchanged when the opt-in knob is absent.
        let candidate_run_bytes = if candidate_use_store_run {
            fs::read(std::path::Path::new(&candidate_store_root).join("run.json"))
                .expect("H2H_CANDIDATE_STORE_ROOT/run.json must be readable")
        } else {
            let pool_bytes = fs::read(
                candidate_pool_json
                    .as_ref()
                    .expect("candidate pool path must exist on the reconstruction path"),
            )
            .expect("H2H_CANDIDATE_POOL_JSON must be a readable file");
            let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
                .expect("pool.json must decode as OpponentLadderPoolContractV1");
            let candidate_base_seed =
                candidate_base_seed.expect("candidate seed must exist on the reconstruction path");
            match (&init_store, wide, environment_randomization_v2) {
            (Some(dir), false, true) => {
                let initialization =
                    crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_initialization_v1(
                        std::path::Path::new(dir),
                        init_gen,
                    )
                    .expect("stage eval init section");
                crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_init_environment_v2(
                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                    64,
                    4,
                    ladder_updates,
                    2,
                    32,
                    16,
                    1_024,
                    2_048,
                    candidate_base_seed,
                    pool,
                    initialization,
                )
            }
            (Some(dir), false, false) => {
                let initialization =
                    crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_initialization_v1(
                        std::path::Path::new(dir),
                        init_gen,
                    )
                    .expect("stage eval init section");
                crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2(
                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                    64,
                    4,
                    ladder_updates,
                    2,
                    32,
                    16,
                    1_024,
                    2_048,
                    candidate_base_seed,
                    pool,
                    initialization,
                )
            }
            (None, false, true) => {
                crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_and_base_seed_ladder_environment_v2(
                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                    64,
                    4,
                    ladder_updates,
                    2,
                    32,
                    16,
                    1_024,
                    2_048,
                    candidate_base_seed,
                    pool,
                )
            }
            (None, false, false) => test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
                NativeTrainingNumericalBackendV1::CudaBurnDense,
                64,
                4,
                ladder_updates,
                2,
                32,
                16,
                1_024,
                2_048,
                candidate_base_seed,
                pool,
            ),
            (None, true, false) => test_fixture_bytes_with_schedule_and_base_seed_wide_ladder_v2(
                NativeTrainingNumericalBackendV1::CudaBurnDense,
                64,
                4,
                ladder_updates,
                2,
                32,
                16,
                1_024,
                2_048,
                candidate_base_seed,
                pool,
            ),
                (Some(_), true, _) | (None, true, true) => unreachable!("guarded above"),
            }
        };
        let candidate_run =
            decode_train_run_v2(&candidate_run_bytes).expect("candidate run record");
        let candidate_root =
            ValidatedNativeTrainingStoreRootV2::open_v2(&candidate_store_root).unwrap();
        let candidate_boundary =
            load_native_training_boundary_v2(&candidate_root, &candidate_run, candidate_gen)
                .unwrap();

        // Opponent: loaded DIRECTLY from a full Store root (digest
        // re-validation optional, eval-only tooling -- see docs above).
        //
        // H2H_OPPONENT_GEN (added 2026-07-25 after the promoted(2)
        // mismeasurement disclosure): when set, the opponent checkpoint is
        // loaded at EXACTLY that generation through the same validated
        // boundary walk the candidate side uses, instead of the store's
        // latest. Unset preserves the original latest-loading behavior
        // byte-for-byte. Rationale: a pool member whose PINNED generation
        // differs from its store HEAD (first instance: pool3 primary,
        // pinned g384, head g512) is silently mismeasured by latest-loading;
        // every h2h invocation SHOULD pin this knob explicitly. The resolved
        // opponent generation is always printed below so logs self-describe
        // either way.
        let opponent_run_bytes =
            fs::read(std::path::Path::new(&opponent_store_root).join("run.json"))
                .expect("H2H_OPPONENT_STORE_ROOT/run.json must be readable");
        let opponent_run = decode_train_run_v2(&opponent_run_bytes).expect("opponent run record");
        let opponent_root =
            ValidatedNativeTrainingStoreRootV2::open_v2(&opponent_store_root).unwrap();
        let opponent_state =
            validate_native_training_store_v2(&opponent_root, &opponent_run).unwrap();
        let opponent_gen_knob: Option<u64> = std::env::var("H2H_OPPONENT_GEN")
            .ok()
            .map(|value| value.parse().expect("H2H_OPPONENT_GEN u64"));
        let opponent_boundary;
        let (opponent_checkpoint, opponent_payload) = match opponent_gen_knob {
            Some(generation) => {
                opponent_boundary =
                    load_native_training_boundary_v2(&opponent_root, &opponent_run, generation)
                        .unwrap();
                (opponent_boundary.checkpoint(), opponent_boundary.payload())
            }
            None => (
                opponent_state.latest_checkpoint(),
                opponent_state.latest_payload(),
            ),
        };
        println!(
            "H2H opponent_resolved_gen={} pinned={}",
            opponent_checkpoint.generation_index(),
            opponent_gen_knob.is_some()
        );
        println!("H2H envrand_v2={environment_randomization_v2}");
        let primary = load_native_checkpoint_inference_v1(
            &opponent_run,
            opponent_checkpoint,
            opponent_payload,
        )
        .unwrap();
        let predecessor_a = load_native_checkpoint_inference_v1(
            &opponent_run,
            opponent_checkpoint,
            opponent_payload,
        )
        .unwrap();
        let predecessor_b = load_native_checkpoint_inference_v1(
            &opponent_run,
            opponent_checkpoint,
            opponent_payload,
        )
        .unwrap();
        let engine = Arc::new(LadderOpponentEngineV1::head_to_head_eval_v1(
            primary,
            predecessor_a,
            predecessor_b,
        ));

        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: eval_seed,
            first_episode_index: 0,
            episode_count,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };
        let result = if wide {
            run_native_checkpoint_wide_with_ladder_opponent_eval_v1(
                &candidate_run,
                candidate_boundary.checkpoint(),
                candidate_boundary.payload(),
                runner_config,
                Some(engine),
            )
            .unwrap()
        } else {
            run_native_checkpoint_with_ladder_opponent_eval_v1(
                &candidate_run,
                candidate_boundary.checkpoint(),
                candidate_boundary.payload(),
                runner_config,
                Some(engine),
            )
            .unwrap()
        };

        // Leg-level W/L/D (2 * pairs games total), matching this file's
        // established print convention for every other saturation probe.
        let episodes = &result.rollout().episodes;
        let bindings = result.episode_bindings();
        assert_eq!(episodes.len(), bindings.len());
        assert_eq!(episodes.len() as u64, episode_count);
        let mut wins = 0_u64;
        let mut losses = 0_u64;
        let mut draws = 0_u64;
        let mut seat_wins = [0_u64; 2];
        let mut seat_losses = [0_u64; 2];
        let mut seat_draws = [0_u64; 2];
        let mut outcome_rows = Vec::with_capacity(episodes.len());
        // PAIR-level win count: Amendment 1 / Section 8A point 1 REJECTS
        // this net-positive-per-pair metric as the gate quantity ("at true
        // parity under CRN seat symmetry, winning both legs of a pair is
        // structurally rare, and that metric misrepresents parity as
        // collapse"); the gate's real denominator is the raw GAME count
        // (`wins` above, tabulated at leg granularity), fed to the gate
        // function below. `pair_wins` is kept ONLY as a labeled diagnostic.
        // A pair is a "diagnostic win" iff the learner's summed reward
        // across its two seat-swapped legs (sharing one environment seed)
        // is net positive; net-zero (a true draw on both legs, or a win
        // cancelled by a loss across the seat swap) and net-negative pairs
        // both fall into "not a diagnostic win".
        let mut pair_wins = 0_u64;
        for pair_offset in 0..pairs {
            let mut pair_reward = 0_i32;
            for leg in 0..2_u64 {
                let index = usize::try_from(pair_offset * 2 + leg).unwrap();
                let binding = &bindings[index];
                let episode = &episodes[index];
                assert_eq!(
                    episode.terminal.terminal_classification,
                    TerminalClassificationV1::Natural
                );
                assert_eq!(
                    episode.terminal.terminal_code,
                    TerminalSafeCodeV2::NaturalGameOver
                );
                assert!(terminal_tuple_is_valid_v1(
                    episode.terminal.terminal_outcome,
                    episode.terminal.terminal_classification,
                    episode.terminal.winner,
                    episode.terminal.terminal_reward,
                ));
                let seat_index = match binding.learner_seat() {
                    PlayerSeatV1::P0 => 0,
                    PlayerSeatV1::P1 => 1,
                };
                let reward = episode.terminal.terminal_reward[seat_index];
                match reward {
                    1 => {
                        wins += 1;
                        seat_wins[seat_index] += 1;
                    }
                    -1 => {
                        losses += 1;
                        seat_losses[seat_index] += 1;
                    }
                    0 => {
                        draws += 1;
                        seat_draws[seat_index] += 1;
                    }
                    other => panic!("unexpected learner reward {other} at a natural terminal"),
                }
                outcome_rows.push(serde_json::json!({
                    "episode_index": binding.episode_index(),
                    "pair_index": pair_offset,
                    "environment_seed": binding.environment_seed(),
                    "learner_seat": if seat_index == 0 { "P0" } else { "P1" },
                    "deck_hashes_u64": binding.deck_hashes(),
                    "opponent_pool_member": "Primary",
                    "terminal_order_rank": reward,
                }));
                pair_reward += reward;
            }
            if pair_reward > 0 {
                pair_wins += 1;
            }
        }
        let total = wins + losses + draws;
        assert_eq!(total, episode_count);
        assert_eq!(
            seat_wins[0] + seat_losses[0] + seat_draws[0],
            pairs,
            "learner-as-P0 total must equal H2H_PAIRS"
        );
        assert_eq!(
            seat_wins[1] + seat_losses[1] + seat_draws[1],
            pairs,
            "learner-as-P1 total must equal H2H_PAIRS"
        );
        println!(
            "H2H candidate_gen={candidate_gen} wide={wide} W/L/D {wins}/{losses}/{draws} of {total}"
        );
        println!("H2H candidate_use_store_run={candidate_use_store_run}");
        for (seat_label, seat) in [("P0", 0_usize), ("P1", 1_usize)] {
            let seat_total = seat_wins[seat] + seat_losses[seat] + seat_draws[seat];
            println!(
                "H2H candidate_gen={candidate_gen} wide={wide} learner_seat={seat_label} W/L/D {}/{}/{} of {seat_total}",
                seat_wins[seat], seat_losses[seat], seat_draws[seat]
            );
        }
        // Labeled diagnostic only (Amendment 1 / Section 8A point 1 rejects
        // this as the gate quantity); NOT fed to the gate function below.
        println!(
            "H2H candidate_gen={candidate_gen} wide={wide} pair_diagnostic_net_positive pair_wins={pair_wins}/{pairs}={:.4}",
            pair_wins as f64 / pairs as f64
        );

        // Amendment 1 / Section 8A point 1: feed the tabulated RAW GAME
        // win count through the actual gate function (win-rate sub-check
        // only; the regression sub-check needs the previous rung's panel
        // mean, computed by the caller from multiple invocations of the
        // panel probe, not from one head-to-head run alone).
        let win_rate_passes = promotion_gate_win_rate_passes_v1(wins, total);
        println!(
            "H2H candidate_gen={candidate_gen} wide={wide} win_rate_sub_check game_wins={wins}/{total}={:.4} passes={win_rate_passes}",
            wins as f64 / total as f64
        );

        // Optional eval-only machine-readable terminal stream. The file is
        // create-new so an interrupted or repeated measurement can never
        // replace earlier outcomes. It contains only terminal W/D/L rank and
        // the already-validated CRN binding needed to compare two independently
        // completed arms. No nonterminal reward or gameplay proxy is emitted.
        if let Some(outcome_path) = std::env::var_os("H2H_OUTCOME_JSON") {
            use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
            use std::fs::OpenOptions;
            use std::io::Write;

            assert!(result.rollout().all_natural());
            let artifact = serde_json::json!({
                "schema": "mtg-kernel-head-to-head-terminal-stream/v1",
                "evaluation_base_seed": eval_seed,
                "pair_count": pairs,
                "episode_count": episode_count,
                "candidate": {
                    "run_sha256": lower_hex_raw32_v1(result.run_sha256()),
                    "identity_bundle_sha256": lower_hex_raw32_v1(result.identity_bundle_sha256()),
                    "generation": candidate_gen,
                    "checkpoint_manifest_sha256": lower_hex_raw32_v1(result.checkpoint_manifest_sha256()),
                    "checkpoint_payload_sha256": lower_hex_raw32_v1(candidate_boundary.checkpoint().checkpoint_payload_sha256()),
                    "model_parameter_sha256": lower_hex_raw32_v1(candidate_boundary.checkpoint().model_parameter_sha256()),
                },
                "opponent": {
                    "run_sha256": opponent_checkpoint.run_sha256(),
                    "generation": opponent_checkpoint.generation_index(),
                    "checkpoint_manifest_sha256": lower_hex_raw32_v1(opponent_checkpoint.checkpoint_manifest_sha256()),
                    "checkpoint_payload_sha256": lower_hex_raw32_v1(opponent_checkpoint.checkpoint_payload_sha256()),
                    "model_parameter_sha256": lower_hex_raw32_v1(opponent_checkpoint.model_parameter_sha256()),
                },
                "runtime": {
                    "worker_count": result.worker_count(),
                    "sessions_per_worker": result.sessions_per_worker(),
                    "broker_batch_target": result.broker_batch_target(),
                    "environment_randomization_v2": environment_randomization_v2,
                    "all_natural": true,
                },
                "learner_outcomes": {
                    "overall": {"wins": wins, "losses": losses, "draws": draws},
                    "P0": {"wins": seat_wins[0], "losses": seat_losses[0], "draws": seat_draws[0]},
                    "P1": {"wins": seat_wins[1], "losses": seat_losses[1], "draws": seat_draws[1]},
                },
                "episodes": outcome_rows,
            });
            let mut bytes = serde_json::to_vec_pretty(&artifact)
                .expect("head-to-head terminal stream must serialize");
            bytes.push(b'\n');
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&outcome_path)
                .expect("H2H_OUTCOME_JSON must name a create-new file");
            file.write_all(&bytes)
                .expect("head-to-head terminal stream must write completely");
            file.sync_all()
                .expect("head-to-head terminal stream must reach the filesystem");
            println!(
                "H2H outcome_artifact={} sha256={}",
                std::path::Path::new(&outcome_path).display(),
                lower_hex_raw32_v1(sha256_v1(&bytes))
            );
        }
    }

    /// Evaluation-only terminal stream for the frozen six-slot response target.
    /// One population component is selected per seat-swapped pair and recorded
    /// on both natural terminal rows. The same evaluation seed therefore gives
    /// the analyzer an exact CRN component, environment, and seat binding across
    /// independently completed candidate and control arms.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn response_exploiter_mixture_eval_v1() {
        use crate::native_checkpoint_runner_v1::run_native_checkpoint_with_population_opponent_eval_v1;
        use crate::native_population_refresh_manifest_v1::decode_population_refresh_manifest_v1;
        use crate::native_population_runtime_resolution_v1::resolve_population_response_target_pairwise_v1;
        use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
        use crate::rl::{
            terminal_tuple_is_valid_v1, PlayerSeatV1, TerminalClassificationV1,
            TerminalSafeCodeV2,
        };
        use std::fs::OpenOptions;
        use std::io::Write;

        const TARGET_REFRESH_SHA256_V1: &str =
            "9c9490b205b7b5a933eae7ca86916e5ff5ff9307a150dc35487a8e1c28e73e22";

        fn required_env_v1(name: &str) -> String {
            std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
        }

        let candidate_store_root = required_env_v1("RESPONSE_H2H_CANDIDATE_STORE_ROOT");
        let candidate_gen: u64 = required_env_v1("RESPONSE_H2H_CANDIDATE_GEN")
            .parse()
            .expect("response candidate generation");
        let diagnostic_v1 = std::env::var("RESPONSE_H2H_DIAGNOSTIC_V1")
            .map(|value| value == "1")
            .unwrap_or(false);
        // Build v2 (probe stage) adds the every-32 grid {0,32,64,96,128,
        // 160,192,224,256}, a superset of the v1 every-64 diagnostic grid
        // {0,64,128,192,256}. Build v2 (confirmation stage) selects
        // whichever of those nine generations the probe stage picked for a
        // given seed, so the non-diagnostic allowlist widens to the same
        // nine-value set (superset of the v1 {0,256}).
        //
        // De-novo screen Phase 2 horizon amendment
        // (CLAUDE-DENOVO-SCREEN-SHEET-V1.md): the 512-update probe's own
        // every-64 grid {0,64,128,192,256,320,384,448,512} is unioned into
        // the diagnostic allowlist only (the denovo screens always run
        // diagnostic; the non-diagnostic build v2 confirmation branch below
        // never reads a denovo candidate and is left untouched).
        if diagnostic_v1 {
            assert!(
                matches!(
                    candidate_gen,
                    0 | 32 | 64 | 96 | 128 | 160 | 192 | 224 | 256 | 320 | 384 | 448 | 512
                ),
                "diagnostic mixture arm must use the shared genesis control or a retained checkpoint"
            );
        } else {
            assert!(
                matches!(candidate_gen, 0 | 32 | 64 | 96 | 128 | 160 | 192 | 224 | 256),
                "mixture arm must be a complete exploiter or its exact promoted(2) genesis control"
            );
        }
        let pairs: u64 = std::env::var("RESPONSE_H2H_PAIRS")
            .unwrap_or_else(|_| "1024".to_owned())
            .parse()
            .expect("RESPONSE_H2H_PAIRS");
        // Build v2 probe stage runs the real diagnostic panel at 256 pairs
        // (not the v1 512) and additionally permits cheap smoke pair counts
        // below 256 for plumbing tests isolated under their own smoke
        // evidence roots. The v1 fixed 512 stays valid.
        if diagnostic_v1 {
            assert!(
                (1..=512).contains(&pairs),
                "diagnostic mixture panel must be the frozen 512, the build v2 probe's 256, or a smaller smoke pair count"
            );
        } else {
            assert_eq!(pairs, 1_024, "frozen mixture panel has 1,024 pairs");
        }
        let eval_seed: u64 = required_env_v1("RESPONSE_H2H_EVAL_SEED")
            .parse()
            .expect("RESPONSE_H2H_EVAL_SEED");
        // Build v2 probe stage uses seeds 1978001 (initial) / 1978011
        // (retry-only) alongside the v1 diagnostic seed 1973001; build v2
        // confirmation stage uses 1976001 (initial) / 1976011 (retry)
        // alongside the v1 frozen seeds 1971001 / 1971011.
        if diagnostic_v1 {
            assert!(
                matches!(
                    eval_seed,
                    1_973_001 | 1_978_001 | 1_978_011 | 1_978_101
                ),
                "diagnostic mixture seed must be the frozen 1973001, a build v2 probe seed, or a denovo-screen probe seed"
            );
        } else {
            assert!(
                matches!(eval_seed, 1_971_001 | 1_971_011 | 1_976_001 | 1_976_011),
                "mixture seed must be a frozen initial/retry seed or a build v2 confirmation seed"
            );
        }
        let outcome_path = required_env_v1("RESPONSE_H2H_OUTCOME_JSON");

        let chain_paths: Vec<PathBuf> = required_env_v1("RESPONSE_H2H_REFRESH_CHAIN")
            .split(';')
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .collect();
        assert_eq!(chain_paths.len(), 9, "refresh-008 requires its full chain");
        let mut chain = Vec::with_capacity(chain_paths.len());
        for path in chain_paths {
            let bytes = fs::read(path).expect("response refresh manifest must be readable");
            let manifest = decode_population_refresh_manifest_v1(&bytes, chain.last())
                .expect("response refresh chain must validate");
            chain.push(manifest);
        }
        let active = chain.last().expect("response refresh chain is nonempty");
        assert_eq!(active.refresh_index_v1(), 8);
        assert_eq!(active.program_update_v1(), 1_024);
        assert_eq!(active.global_generation_v1(), 1_536);
        assert_eq!(
            lower_hex_raw32_v1(active.manifest_sha256_v1()),
            TARGET_REFRESH_SHA256_V1
        );

        let slot_roots: Vec<PathBuf> = required_env_v1("RESPONSE_H2H_SLOT_ROOTS")
            .split(';')
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .collect();
        let population_engine = Arc::new(
            resolve_population_response_target_pairwise_v1(active, &slot_roots)
                .expect("response mixture target must resolve through all Store authorities"),
        );

        let candidate_run_bytes =
            fs::read(Path::new(&candidate_store_root).join("run.json"))
                .expect("response candidate run.json must be readable");
        let candidate_run =
            decode_train_run_v2(&candidate_run_bytes).expect("response candidate RunV2");
        let candidate_root =
            ValidatedNativeTrainingStoreRootV2::open_v2(&candidate_store_root).unwrap();
        let candidate_boundary =
            load_native_training_boundary_v2(&candidate_root, &candidate_run, candidate_gen)
                .unwrap();

        let episode_count = pairs.checked_mul(2).expect("pair count overflow");
        let result = run_native_checkpoint_with_population_opponent_eval_v1(
            &candidate_run,
            candidate_boundary.checkpoint(),
            candidate_boundary.payload(),
            NativeCheckpointRunnerConfigV1 {
                evaluation_base_seed: eval_seed,
                first_episode_index: 0,
                episode_count,
                scheduler_timeout: Duration::from_secs(3_600),
                measure_broker_service_time: false,
            },
            population_engine.clone(),
        )
        .unwrap();
        assert!(result.rollout().all_natural());

        let episodes = &result.rollout().episodes;
        let bindings = result.episode_bindings();
        assert_eq!(episodes.len(), usize::try_from(episode_count).unwrap());
        assert_eq!(episodes.len(), bindings.len());
        let mut selected = population_engine.selected_episode_slots_for_test_v1();
        selected.sort_unstable_by_key(|(_, episode_index, _)| *episode_index);
        assert_eq!(selected.len(), episodes.len());

        let mut wins = 0_u64;
        let mut losses = 0_u64;
        let mut draws = 0_u64;
        let mut seat_wins = [0_u64; 2];
        let mut seat_losses = [0_u64; 2];
        let mut seat_draws = [0_u64; 2];
        let mut outcome_rows = Vec::with_capacity(episodes.len());
        for pair_index in 0..pairs {
            let first_index = usize::try_from(pair_index * 2).unwrap();
            let second_index = first_index + 1;
            assert_eq!(selected[first_index].0, eval_seed);
            assert_eq!(selected[second_index].0, eval_seed);
            assert_eq!(selected[first_index].1, pair_index * 2);
            assert_eq!(selected[second_index].1, pair_index * 2 + 1);
            let slot_index = selected[first_index].2;
            assert_eq!(selected[second_index].2, slot_index);
            assert!(slot_index < 6, "excluded exploiter fallback was selected");
            assert_eq!(
                bindings[first_index].environment_seed(),
                bindings[second_index].environment_seed()
            );
            assert_ne!(
                bindings[first_index].learner_seat(),
                bindings[second_index].learner_seat()
            );
            let component = &active.slots_v1()[slot_index];

            for index in [first_index, second_index] {
                let binding = &bindings[index];
                let episode = &episodes[index];
                assert_eq!(
                    episode.terminal.terminal_classification,
                    TerminalClassificationV1::Natural
                );
                assert_eq!(
                    episode.terminal.terminal_code,
                    TerminalSafeCodeV2::NaturalGameOver
                );
                assert!(terminal_tuple_is_valid_v1(
                    episode.terminal.terminal_outcome,
                    episode.terminal.terminal_classification,
                    episode.terminal.winner,
                    episode.terminal.terminal_reward,
                ));
                let seat_index = match binding.learner_seat() {
                    PlayerSeatV1::P0 => 0,
                    PlayerSeatV1::P1 => 1,
                };
                let reward = episode.terminal.terminal_reward[seat_index];
                match reward {
                    1 => {
                        wins += 1;
                        seat_wins[seat_index] += 1;
                    }
                    -1 => {
                        losses += 1;
                        seat_losses[seat_index] += 1;
                    }
                    0 => {
                        draws += 1;
                        seat_draws[seat_index] += 1;
                    }
                    other => panic!("unexpected response candidate reward {other}"),
                }
                outcome_rows.push(serde_json::json!({
                    "episode_index": binding.episode_index(),
                    "pair_index": pair_index,
                    "environment_seed": binding.environment_seed(),
                    "learner_seat": if seat_index == 0 { "P0" } else { "P1" },
                    "deck_hashes_u64": binding.deck_hashes(),
                    "opponent_population_slot": slot_index,
                    "opponent": {
                        "run_sha256": component.source_run_sha256_v1(),
                        "generation": component.source_generation_v1(),
                        "checkpoint_manifest_sha256": component.checkpoint_sha256_v1(),
                        "checkpoint_payload_sha256": component.state_sha256_v1(),
                        "model_parameter_sha256": component.model_parameter_sha256_v1(),
                    },
                    "terminal_order_rank": reward,
                }));
            }
        }
        assert_eq!(wins + losses + draws, episode_count);
        for seat in 0..2 {
            assert_eq!(
                seat_wins[seat] + seat_losses[seat] + seat_draws[seat],
                pairs
            );
        }

        let checkpoint = candidate_boundary.checkpoint();
        let components: Vec<_> = active
            .slots_v1()
            .iter()
            .take(6)
            .map(|component| {
                serde_json::json!({
                    "run_sha256": component.source_run_sha256_v1(),
                    "generation": component.source_generation_v1(),
                    "checkpoint_manifest_sha256": component.checkpoint_sha256_v1(),
                    "checkpoint_payload_sha256": component.state_sha256_v1(),
                    "model_parameter_sha256": component.model_parameter_sha256_v1(),
                })
            })
            .collect();
        let mut artifact = serde_json::json!({
            "schema": "mtg-kernel-response-exploiter-mixture-terminal-stream/v1",
            "evaluation_base_seed": eval_seed,
            "pair_count": pairs,
            "episode_count": episode_count,
            "candidate": {
                "run_sha256": lower_hex_raw32_v1(result.run_sha256()),
                "identity_bundle_sha256": lower_hex_raw32_v1(result.identity_bundle_sha256()),
                "generation": candidate_gen,
                "checkpoint_manifest_sha256": lower_hex_raw32_v1(result.checkpoint_manifest_sha256()),
                "checkpoint_payload_sha256": lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()),
                "model_parameter_sha256": lower_hex_raw32_v1(checkpoint.model_parameter_sha256()),
            },
            "opponent_population": {
                "refresh_sha256": TARGET_REFRESH_SHA256_V1,
                "weights": [125407, 115542, 127252, 127098, 128077, 127916, 0, 0],
                "declared_total": 751292,
                "components": components,
            },
            "runtime": {
                "worker_count": result.worker_count(),
                "sessions_per_worker": result.sessions_per_worker(),
                "broker_batch_target": result.broker_batch_target(),
                "environment_randomization_v2": true,
                "all_natural": true,
            },
            "learner_outcomes": {
                "overall": {"wins": wins, "losses": losses, "draws": draws},
                "P0": {"wins": seat_wins[0], "losses": seat_losses[0], "draws": seat_draws[0]},
                "P1": {"wins": seat_wins[1], "losses": seat_losses[1], "draws": seat_draws[1]},
            },
            "episodes": outcome_rows,
        });
        if diagnostic_v1 {
            artifact["runtime"]["response_exploiter_diagnostic_v1"] = serde_json::json!(true);
        }
        let mut bytes = serde_json::to_vec_pretty(&artifact)
            .expect("response mixture terminal stream must serialize");
        bytes.push(b'\n');
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&outcome_path)
            .expect("RESPONSE_H2H_OUTCOME_JSON must name a create-new file");
        file.write_all(&bytes)
            .expect("response mixture terminal stream must write completely");
        file.sync_all()
            .expect("response mixture terminal stream must reach the filesystem");
        println!(
            "RESPONSE_H2H candidate_gen={candidate_gen} W/L/D {wins}/{losses}/{draws} artifact={} sha256={}",
            Path::new(&outcome_path).display(),
            lower_hex_raw32_v1(sha256_v1(&bytes))
        );
    }

    /// per generation. Non-banked diagnostic; the store root arrives via
    /// PATHFINDING_STORE_ROOT and generations via PATHFINDING_EVAL_GENS
    /// (comma-separated).
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn pathfinding_saturation_eval_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let root_path = std::env::var("PATHFINDING_STORE_ROOT")
            .expect("PATHFINDING_STORE_ROOT must name the surviving store root");
        let generations: Vec<u64> = std::env::var("PATHFINDING_EVAL_GENS")
            .expect("PATHFINDING_EVAL_GENS must list generations")
            .split(',')
            .map(|token| token.trim().parse().expect("generation index"))
            .collect();

        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            8_192,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("pathfinding run record");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&root_path).unwrap();
        let runner_config = NativeCheckpointRunnerConfigV1 {
            evaluation_base_seed: 7_777,
            first_episode_index: 0,
            episode_count: 256,
            scheduler_timeout: Duration::from_secs(3_600),
            measure_broker_service_time: false,
        };

        let reference_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let reference_run = run_native_checkpoint_v1(
            &run,
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
            runner_config,
        )
        .unwrap();
        for generation in generations {
            let boundary = load_native_training_boundary_v2(&root, &run, generation).unwrap();
            let candidate_run = run_native_checkpoint_v1(
                &run,
                boundary.checkpoint(),
                boundary.payload(),
                runner_config,
            )
            .unwrap();
            let evaluation =
                evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run)
                    .unwrap();
            let outcomes = evaluation.candidate_learner_outcomes();
            println!(
                "SATURATION gen={generation} W/L/D {}/{}/{} of {} (delta vs gen0 {})",
                outcomes.wins(),
                outcomes.losses(),
                outcomes.draws(),
                outcomes.total(),
                evaluation.total_candidate_minus_reference_reward_delta()
            );
        }
    }

    /// Overnight non-banked pathfinding run: K=64 vs uniform on the
    /// CudaBurnDense backend in record-only measurement mode to thousands of
    /// updates. Purpose: uniform-opponent saturation depth, durable-store
    /// behavior at real depth, and drift trajectory far past the measured
    /// 256-update horizon. Wall-clock and throughput from this probe are
    /// explicitly non-evidence (it runs concurrent with other lanes); the
    /// per-update QUALIFICATION_JSONL drift series and the store artifacts
    /// are its products. Direct the store under a roomy drive via TMP.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn pathfinding_run_k64_deep_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let _measurement_mode = MeasurementModeGuardV1::arm();

        let updates = 8_192_u64;
        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            updates,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("pathfinding run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut execution_config = test_execution_config_v2(&run);
        execution_config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;

        let parent = TestParentV1::new("pathfinding-deep");
        println!("pathfinding store parent: {}", parent.parent.display());
        let bootstrapped =
            crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                &parent.parent,
                "store",
            )
            .unwrap();
        let root = bootstrapped.into_root();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config.clone(),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let genesis_receipt = publish_genesis_generation_v2(
            &root,
            &run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .unwrap();
        assert_eq!(genesis_receipt.generation_index(), 0);

        let started = std::time::Instant::now();
        let mut committed_segments = 0_u64;
        loop {
            match resume_native_training_store_v2(&root, &run, execution_config.clone()) {
                Ok(NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                }) => {
                    println!(
                        "pathfinding: COMPLETE at generation {latest_generation_index} \
                         after {:.0}s",
                        started.elapsed().as_secs_f64()
                    );
                    break;
                }
                Ok(NativeTrainingStoreResumeV2::Continue(mut continuation)) => {
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .expect("pathfinding prepare");
                    let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                    committed_segments += 1;
                    if committed_segments.is_multiple_of(64) {
                        println!(
                            "pathfinding: {} updates committed, {:.0}s elapsed, \
                             {:.2} eps/s cumulative (non-evidence)",
                            committed_segments * 4,
                            started.elapsed().as_secs_f64(),
                            (committed_segments * 4 * 64) as f64 / started.elapsed().as_secs_f64()
                        );
                    }
                }
                Err(error) => panic!("pathfinding resume: {error:?}"),
            }
        }
    }

    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn qualification_measurement_k64_depth256_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let _measurement_mode = MeasurementModeGuardV1::arm();

        let updates = 256_u64;
        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            updates,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("measurement run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut execution_config = test_execution_config_v2(&run);
        execution_config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;

        let parent = TestParentV1::new("qualification-depth256");
        let bootstrapped =
            crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                &parent.parent,
                "store",
            )
            .unwrap();
        let root = bootstrapped.into_root();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config.clone(),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let genesis_receipt = publish_genesis_generation_v2(
            &root,
            &run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .unwrap();
        assert_eq!(genesis_receipt.generation_index(), 0);

        loop {
            match resume_native_training_store_v2(&root, &run, execution_config.clone()) {
                Ok(NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                }) => {
                    println!("measurement: COMPLETE at generation {latest_generation_index}");
                    break;
                }
                Ok(NativeTrainingStoreResumeV2::Continue(mut continuation)) => {
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .expect("measurement prepare");
                    let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                }
                Err(error) => panic!("measurement resume: {error:?}"),
            }
        }
    }

    /// Diagnostic twin of the learning smoke: same K=64 x 128-update run
    /// driven through the manual bootstrap/genesis/resume loop so the
    /// underlying trainer error is printed with full detail instead of the
    /// loop's redacted phase classification. Reports the failing update
    /// window by counting successful segment commits.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn learning_smoke_k64_failure_diagnostic_v1() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::test_fixture_bytes_with_schedule_v2;

        let updates = 128_u64;
        let patched = test_fixture_bytes_with_schedule_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            updates,
            8,
            8,
            32,
            1_024,
            2_048,
        );
        let run = decode_train_run_v2(&patched).expect("diagnostic run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let mut execution_config = test_execution_config_v2(&run);
        execution_config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;

        let parent = TestParentV1::new("learning-smoke-diag");
        let bootstrapped =
            crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                &parent.parent,
                "store",
            )
            .unwrap();
        let root = bootstrapped.into_root();
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            execution_config.clone(),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let candidate = executor.checkpoint_candidate_v1().unwrap();
        let payload = candidate.payload().to_vec();
        let checkpoint = build_genesis_checkpoint_manifest_v3(&run, &payload).unwrap();
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let genesis_receipt = publish_genesis_generation_v2(
            &root,
            &run,
            &payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .unwrap();
        assert_eq!(genesis_receipt.generation_index(), 0);

        let mut committed_segments = 0_u64;
        loop {
            match resume_native_training_store_v2(&root, &run, execution_config.clone()) {
                Ok(NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                }) => {
                    println!(
                        "diagnostic: COMPLETE at generation {latest_generation_index} \
                         ({committed_segments} segments committed this process)"
                    );
                    break;
                }
                Ok(NativeTrainingStoreResumeV2::Continue(mut continuation)) => {
                    let prepared = match prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    ) {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            panic!(
                                "diagnostic: prepare FAILED after {committed_segments} committed \
                                 segments (updates {}..{}): {error:?}",
                                committed_segments * 4,
                                committed_segments * 4 + 4,
                            );
                        }
                    };
                    let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                    committed_segments += 1;
                    if committed_segments.is_multiple_of(8) {
                        println!(
                            "diagnostic: {committed_segments} segments \
                             ({} updates) committed",
                            committed_segments * 4
                        );
                    }
                }
                Err(error) => {
                    panic!(
                        "diagnostic: resume FAILED after {committed_segments} committed \
                         segments: {error:?}"
                    );
                }
            }
        }
    }

    /// Temporary GPU K-scaling measurement probe: end-to-end durable training
    /// throughput at K = 2/16/64/256 with the CudaBurnDense train-step backend
    /// and the same scaled topology grid as the CPU probe, one segment
    /// (S=4, N=4) per configuration, cold-start inclusive.
    #[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn timing_probe_gpu_k_scaling_throughput() {
        use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_v2, ValidatedTrainRunV2,
        };

        fn cuda_execution_config_v2(
            run: &ValidatedTrainRunV2,
        ) -> crate::native_training_executor_v1::NativeTrainingExecutionConfigV1 {
            let mut config = test_execution_config_v2(run);
            config.numerical_backend = NativeTrainingNumericalBackendV1::CudaBurnDense;
            config
        }

        let configurations: [(u64, u64, u64, u64, u64, u64); 4] = [
            // (K, workers, sessions, broker, max_physical, max_policy)
            (2, 0, 0, 0, 32_768, 65_536),
            (16, 4, 4, 8, 2_048, 4_096),
            (64, 8, 8, 32, 1_024, 2_048),
            (256, 16, 16, 128, 2_048, 4_096),
        ];
        let updates = 4_u64;

        for (batch_episodes, workers, sessions, broker, max_physical, max_policy) in configurations
        {
            let patched = if workers == 0 {
                test_fixture_bytes_with_schedule_v2(
                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                    batch_episodes,
                    4,
                    updates,
                    1,
                    2,
                    2,
                    max_physical,
                    max_policy,
                )
            } else {
                test_fixture_bytes_with_schedule_v2(
                    NativeTrainingNumericalBackendV1::CudaBurnDense,
                    batch_episodes,
                    4,
                    updates,
                    workers,
                    sessions,
                    broker,
                    max_physical,
                    max_policy,
                )
            };
            let run = match decode_train_run_v2(&patched) {
                Ok(run) => run,
                Err(error) => {
                    panic!("K={batch_episodes}: run record rejected: {error}");
                }
            };
            let episodes = batch_episodes * updates;
            let target = run.requested_successful_updates();

            let parent = TestParentV1::new("gpu-kscale");
            let started = std::time::Instant::now();
            let bootstrapped =
                crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                    &parent.parent,
                    "store",
                )
                .unwrap();
            let root = bootstrapped.into_root();
            let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
            let executor = crate::native_training_executor_v1::NativeTrainingExecutorV1::
                from_common_model_snapshot_v1(
                    cuda_execution_config_v2(&run),
                    &snapshot_manifest,
                    &snapshot_payload,
                )
                .unwrap();
            let candidate = executor.checkpoint_candidate_v1().unwrap();
            let payload = candidate.payload().to_vec();
            let checkpoint =
                crate::native_training_store_checkpoint_v3::build_genesis_checkpoint_manifest_v3(
                    &run, &payload,
                )
                .unwrap();
            let segment = crate::native_training_store_segment_manifest_v2::
                build_genesis_segment_manifest_v2(&run, &checkpoint)
            .unwrap();
            let boundary = crate::native_training_store_boundary_v2::
                build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint)
            .unwrap();
            let reference =
                crate::native_training_store_reference_latest_v2::build_checkpoint_reference_v2(
                    &run, &boundary,
                )
                .unwrap();
            let latest = crate::native_training_store_reference_latest_v2::build_latest_v2(
                &boundary, &reference,
            )
            .unwrap();
            let _ = crate::native_training_store_v2::publish_genesis_generation_v2(
                &root,
                &run,
                &payload,
                &checkpoint,
                &segment,
                &boundary,
                &reference,
                &latest,
            )
            .unwrap();
            let genesis_done = started.elapsed().as_secs_f64();

            let mut train_result = Ok(());
            loop {
                match crate::native_training_store_resume_v2::resume_native_training_store_v2(
                    &root,
                    &run,
                    cuda_execution_config_v2(&run),
                ) {
                    Ok(
                        crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2::Complete {
                            latest_generation_index,
                        },
                    ) => {
                        assert_eq!(latest_generation_index, target);
                        break;
                    }
                    Ok(
                        crate::native_training_store_resume_v2::NativeTrainingStoreResumeV2::Continue(
                            mut continuation,
                        ),
                    ) => {
                        let prepared = match crate::native_training_store_prepared_segment_v2::
                            prepare_segment_v2(
                                &mut continuation.executor,
                                &run,
                                &continuation.parent_boundary,
                                &continuation.parent_checkpoint,
                            ) {
                            Ok(prepared) => prepared,
                            Err(error) => {
                                train_result = Err(format!("prepare: {error}"));
                                break;
                            }
                        };
                        let receipt = crate::native_training_store_v2::
                            publish_prepared_segment_v2(
                                &root,
                                &run,
                                &continuation.parent_boundary,
                                &continuation.parent_checkpoint,
                                &prepared,
                            )
                            .unwrap();
                        prepared.commit_v2(receipt).unwrap();
                    }
                    Err(error) => {
                        train_result = Err(format!("resume: {error}"));
                        break;
                    }
                }
            }
            let wall = started.elapsed().as_secs_f64();
            match train_result {
                Ok(()) => {
                    let rate = episodes as f64 / wall;
                    println!(
                        "K={batch_episodes}: {episodes} episodes over {wall:.3}s \
                         (genesis {genesis_done:.3}s) = {rate:.4} eps/s \
                         [vs floor 0.2925: {:.1}x]",
                        rate / 0.2925
                    );
                }
                Err(message) => {
                    panic!("K={batch_episodes}: training failed after {wall:.3}s: {message}");
                }
            }
        }
    }

    /// A crash between run-authority and latest publication leaves run.json
    /// present with latest.json never written (bootstrap B8). The loop must
    /// recover by retrying genesis and then train to the exact target, per
    /// the frozen draft's train-new recovery clause. The state is
    /// constructed literally: bootstrap the empty skeleton, then write the
    /// run's exact canonical bytes as run.json, which is byte-identical to
    /// what the interrupted publisher leaves behind.
    #[test]
    fn science_loop_recovers_interrupted_genesis() {
        let parent = TestParentV1::new("genesis-recovery");
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let target = run.requested_successful_updates();

        let bootstrapped =
            crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2(
                &parent.parent,
                "store",
            )
            .unwrap();
        assert!(matches!(
            bootstrapped.outcome(),
            crate::native_training_store_bootstrap_v2::NativeTrainingStoreBootstrapOutcomeV2::SkeletonReady
        ));
        let root = bootstrapped.into_root();
        fs::write(root.root_path().join("run.json"), run.canonical_bytes()).unwrap();
        drop(root);

        let report = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(report.latest_generation_index(), target);
        assert_eq!(report.candidate_run().generation_index(), target);
    }

    #[test]
    fn genesis_publisher_mapping_preserves_global_busy_and_platform_errors() {
        assert_eq!(
            map_genesis_publisher_error_kind_v1(
                NativeTrainingStorePublisherV2ErrorKind::UnsupportedPlatform
            )
            .kind(),
            NativeScienceLoopV1ErrorKind::UnsupportedPlatform
        );
        assert_eq!(
            map_genesis_publisher_error_kind_v1(NativeTrainingStorePublisherV2ErrorKind::StoreBusy)
                .kind(),
            NativeScienceLoopV1ErrorKind::StoreBusy
        );
        for kind in [
            NativeTrainingStorePublisherV2ErrorKind::RootInvalid,
            NativeTrainingStorePublisherV2ErrorKind::InputInvalid,
            NativeTrainingStorePublisherV2ErrorKind::StageCorruption,
            NativeTrainingStorePublisherV2ErrorKind::PublicationFailed,
            NativeTrainingStorePublisherV2ErrorKind::ImmutableFinalMismatchCorruption,
            NativeTrainingStorePublisherV2ErrorKind::GenerationInvalid,
            NativeTrainingStorePublisherV2ErrorKind::LatestInvalid,
        ] {
            assert_eq!(
                map_genesis_publisher_error_kind_v1(kind).kind(),
                NativeScienceLoopV1ErrorKind::GenesisFailed
            );
        }
    }

    #[test]
    fn one_command_recovers_exact_run_only_and_candidate_equal_partial_genesis() {
        let parent = TestParentV1::new("run-only-recovery");
        let run = one_segment_run_v1();
        let root_path = establish_exact_run_only_v1(&parent, &run);
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();

        // Reproduce a real interrupted publisher state: exact run authority,
        // the first candidate-equal generation-zero immutable, a recognized
        // latest stage, and no latest final.
        let executor = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
        )
        .unwrap();
        let partial_payload = executor
            .checkpoint_candidate_v1()
            .unwrap()
            .payload()
            .to_vec();
        let partial_payload_path = root_path
            .join("checkpoints")
            .join("update-00000000.state.f32le");
        fs::write(&partial_payload_path, &partial_payload).unwrap();
        let latest_stage_path = root_path.join(".latest.json.stage-v2");
        fs::write(&latest_stage_path, b"interrupted-latest-stage").unwrap();

        let report = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(report.latest_generation_index(), 4);
        assert_eq!(report.reference_run().generation_index(), 0);
        assert_eq!(report.candidate_run().generation_index(), 4);
        assert_eq!(fs::read(&partial_payload_path).unwrap(), partial_payload);
        assert!(fs::symlink_metadata(&latest_stage_path).is_err());

        let reopened = bootstrap_native_training_store_v2(&parent.parent, "store").unwrap();
        assert_eq!(
            reopened.outcome(),
            NativeTrainingStoreBootstrapOutcomeV2::RunAuthorityPresent
        );
        assert!(reopened.latest_final_present());
        let state = validate_native_training_store_v2(reopened.root(), &run).unwrap();
        assert_eq!(state.latest_generation_index(), 4);
    }

    #[test]
    fn run_only_mismatching_run_fails_as_genesis_without_mutation() {
        let parent = TestParentV1::new("run-only-mismatching-run");
        let run = one_segment_run_v1();
        let root_path = establish_exact_run_only_v1(&parent, &run);
        let run_path = root_path.join("run.json");
        let mut mismatching_run = run.canonical_bytes().to_vec();
        let flip = mismatching_run.len() / 2;
        mismatching_run[flip] ^= 0x01;
        fs::write(&run_path, &mismatching_run).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();

        let error = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeScienceLoopV1ErrorKind::GenesisFailed);
        assert_eq!(fs::read(run_path).unwrap(), mismatching_run);
        assert!(fs::symlink_metadata(root_path.join("latest.json")).is_err());
        assert_generation_directories_empty_v1(&root_path);
    }

    #[test]
    fn present_but_invalid_latest_is_not_misclassified_as_recoverable_genesis() {
        let parent = TestParentV1::new("invalid-latest");
        let run = one_segment_run_v1();
        let root_path = establish_exact_run_only_v1(&parent, &run);
        let latest_path = root_path.join("latest.json");
        fs::write(&latest_path, b"{}").unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();

        let error = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(error.kind(), NativeScienceLoopV1ErrorKind::TrainFailed);
        assert_eq!(fs::read(latest_path).unwrap(), b"{}");
        assert_generation_directories_empty_v1(&root_path);
    }

    #[test]
    fn one_command_science_loop_trains_runs_evaluates_and_reruns_deterministically() {
        let parent = TestParentV1::new("smoke");
        let run = decode_train_run_v2(&test_fixture_bytes_v2()).unwrap();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let target = run.requested_successful_updates();

        let first = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(first.latest_generation_index(), target);
        assert_eq!(first.reference_run().generation_index(), 0);
        assert_eq!(first.candidate_run().generation_index(), target);
        let evaluation = first.evaluation();
        assert_eq!(evaluation.reference_generation_index(), 0);
        assert_eq!(evaluation.candidate_generation_index(), target);
        assert_eq!(evaluation.pair_count(), 1);
        assert_eq!(evaluation.leg_count(), 2);
        let reference_outcomes = evaluation.reference_learner_outcomes();
        let candidate_outcomes = evaluation.candidate_learner_outcomes();
        assert_eq!(
            reference_outcomes.wins() + reference_outcomes.losses() + reference_outcomes.draws(),
            reference_outcomes.total()
        );
        assert_eq!(
            candidate_outcomes.wins() + candidate_outcomes.losses() + candidate_outcomes.draws(),
            candidate_outcomes.total()
        );

        // The second invocation resumes the completed store as the exact
        // no-op and must reproduce the identical evaluation: same pairs, same
        // rewards, same delta. This is the deterministic science guarantee.
        let second = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(second.latest_generation_index(), target);
        assert_eq!(
            second
                .evaluation()
                .total_candidate_minus_reference_reward_delta(),
            evaluation.total_candidate_minus_reference_reward_delta()
        );
        assert_eq!(
            second.evaluation().reward_pairs().len(),
            evaluation.reward_pairs().len()
        );
        for (second_pair, first_pair) in second
            .evaluation()
            .reward_pairs()
            .iter()
            .zip(evaluation.reward_pairs())
        {
            assert_eq!(second_pair.pair_index(), first_pair.pair_index());
            assert_eq!(
                second_pair.environment_seed(),
                first_pair.environment_seed()
            );
            assert_eq!(
                second_pair.reference_rewards_by_learner_seat(),
                first_pair.reference_rewards_by_learner_seat()
            );
            assert_eq!(
                second_pair.candidate_rewards_by_learner_seat(),
                first_pair.candidate_rewards_by_learner_seat()
            );
        }
    }

    /// GEN-0 AS SELECTION CANDIDATE (Self-Play Ladder Design Contract S2,
    /// Amendment 1 / Section 8A point 2, Deliverable 4): under continual
    /// initialization, generation 0 -- the inherited checkpoint itself --
    /// joins the evaluated selection schedule as a genuine candidate, not
    /// merely the delta reference, so best-panel selection structurally
    /// cannot return worse than the initialization. This proves the
    /// mechanism `ladder_saturation_eval_v1`'s LADDER_EVAL_GENS loop relies
    /// on when `0` is one of the requested generations: loading generation 0
    /// as BOTH the reference and the candidate runs it through the exact
    /// same `run_native_checkpoint_v1` / `evaluate_native_checkpoint_uniform_delta_v1`
    /// path with no special-casing, trips no assert, and produces an exact
    /// zero delta by construction. Exercised against a real (trained)
    /// LADDER-identity store -- the ladder fixture builder, not the uniform
    /// one -- since the probe itself takes its stores from env vars this
    /// test cannot depend on. Uses the real ladder pilot pool.json
    /// (read-only) for a structurally valid pool section; the opponent
    /// engine passed to the trainer is `None` (uniform fallback), since
    /// this test's subject is checkpoint SELECTION machinery, not the
    /// opponent engine.
    #[test]
    fn ladder_gen_zero_as_both_reference_and_candidate_runs_cleanly() {
        use crate::native_training_store_run_v2::{
            test_fixture_bytes_with_schedule_and_base_seed_ladder_v2, OpponentLadderPoolContractV1,
        };

        let pool_bytes = fs::read(r"D:\mtg-kernel-ladder-pilot-20260725\pool\pool.json")
            .expect("real ladder pilot pool.json must be readable");
        let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
            .expect("pool.json must decode as OpponentLadderPoolContractV1");

        let patched = test_fixture_bytes_with_schedule_and_base_seed_ladder_v2(
            NativeTrainingNumericalBackendV1::Sequential,
            2,
            4,
            4,
            2,
            4,
            8,
            32_768,
            65_536,
            555_002,
            pool,
        );
        let run = decode_train_run_v2(&patched).expect("ladder run record");
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let parent = TestParentV1::new("ladder-gen-zero-candidate");

        run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
            None,
        )
        .expect("ladder science loop");
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&parent.parent.join("store"))
            .expect("validated store root");

        // LADDER_EVAL_GENS's own loop body: gen 0 as the reference AND, here,
        // gen 0 again in place of a later `generation` drawn from the
        // requested list.
        let reference_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let reference_run = run_native_checkpoint_v1(
            &run,
            reference_boundary.checkpoint(),
            reference_boundary.payload(),
            runner_config_v1(),
        )
        .unwrap();
        let candidate_boundary = load_native_training_boundary_v2(&root, &run, 0).unwrap();
        let candidate_run = run_native_checkpoint_v1(
            &run,
            candidate_boundary.checkpoint(),
            candidate_boundary.payload(),
            runner_config_v1(),
        )
        .unwrap();
        let evaluation =
            evaluate_native_checkpoint_uniform_delta_v1(&reference_run, &candidate_run).unwrap();
        assert_eq!(evaluation.reference_generation_index(), 0);
        assert_eq!(evaluation.candidate_generation_index(), 0);
        assert_eq!(evaluation.total_candidate_minus_reference_reward_delta(), 0);
        assert_eq!(
            evaluation.reference_learner_outcomes(),
            evaluation.candidate_learner_outcomes()
        );
    }

    /// GenesisInitializationV2 fixture (Self-Play Ladder Design Contract S2,
    /// Amendment 1 / Section 8A point 2, Section 8B): a real ladder-init run
    /// record (`opponent_ladder_initialization` referencing a REAL trained
    /// S1 checkpoint, D:\mtg-kernel-s1-mirror-20260724\dev1\run-0\store
    /// generation 32, read-only source) plus that checkpoint's own resolved
    /// `GenesisInitializationReferenceV2`, resolved through the record's own
    /// init section via `ladder_init_as_checkpoint_ref_v1` (the same
    /// conversion + chain-proven loader a real caller uses, not the staging
    /// step used only to author the record). The pool section reuses the
    /// real, already-published ladder pilot pool.json (read-only);
    /// incidental to genesis, present only because a structurally valid
    /// pool is required alongside the ladder identity.
    struct LadderInitFixtureV1 {
        run_bytes: Vec<u8>,
        reference: GenesisInitializationReferenceV2,
    }

    static LADDER_INIT_FIXTURE_V1: OnceLock<LadderInitFixtureV1> = OnceLock::new();

    const REAL_LADDER_INIT_REFERENCE_STORE_V1: &str =
        r"D:\mtg-kernel-s1-mirror-20260724\dev1\run-0\store";
    const REAL_LADDER_INIT_REFERENCE_GENERATION_V1: u64 = 32;

    fn ladder_init_fixture_v1() -> &'static LadderInitFixtureV1 {
        LADDER_INIT_FIXTURE_V1.get_or_init(|| {
            use crate::native_ladder_pool_resolution_v1::{
                ladder_init_as_checkpoint_ref_v1, resolve_ladder_checkpoint_authority_v1,
                stage_ladder_checkpoint_initialization_v1,
            };
            use crate::native_training_store_run_v2::{
                test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2,
                OpponentLadderPoolContractV1,
            };

            let pool_bytes = fs::read(r"D:\mtg-kernel-ladder-pilot-20260725\pool\pool.json")
                .expect("real ladder pilot pool.json must be readable");
            let pool: OpponentLadderPoolContractV1 = serde_json::from_slice(&pool_bytes)
                .expect("pool.json must decode as OpponentLadderPoolContractV1");

            // Design directive slice 2: stages the complete six-field init
            // section (five-field digest-pin plus
            // `derived_model_parameter_sha256`) from the real reference
            // checkpoint on disk.
            let init = stage_ladder_checkpoint_initialization_v1(
                std::path::Path::new(REAL_LADDER_INIT_REFERENCE_STORE_V1),
                REAL_LADDER_INIT_REFERENCE_GENERATION_V1,
            )
            .expect("real S1 mirror gen-32 checkpoint must stage to a valid init section");

            let run_bytes = test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                2,
                4,
                4,
                2,
                4,
                8,
                32_768,
                65_536,
                555_030,
                pool,
                init,
            );
            let run = decode_train_run_v2(&run_bytes).expect("ladder-init run record");

            let checkpoint_ref = ladder_init_as_checkpoint_ref_v1(
                run.record()
                    .contracts
                    .opponent_ladder_initialization
                    .as_ref()
                    .expect("fixture record must carry the init section"),
            );
            let authority = resolve_ladder_checkpoint_authority_v1(
                std::path::Path::new(REAL_LADDER_INIT_REFERENCE_STORE_V1),
                &checkpoint_ref,
            )
            .expect("real S1 mirror checkpoint must resolve through the chain-proven loader");
            let (checkpoint, payload) = authority.into_checkpoint_and_payload();

            LadderInitFixtureV1 {
                run_bytes,
                reference: GenesisInitializationReferenceV2 {
                    checkpoint,
                    payload,
                },
            }
        })
    }

    /// THE BIT-EQUALITY PROOF, in-memory half (Self-Play Ladder Design
    /// Contract S2, Amendment 1 / Section 8A point 2, Section 8B, mandatory
    /// test list): builds and decodes generation 0 through
    /// GenesisInitializationV2 and proves the WEIGHT region bit-equals the
    /// source checkpoint's own weights while every moment region (first and
    /// second) is exact positive zero. In-memory only (no Store I/O) -- see
    /// `ladder_init_genesis_publishes_through_the_pure_walk_reconciler`
    /// immediately below for the same proof carried through an actual
    /// publish and independent walk revalidation, and the module-level
    /// `native_training_store_checkpoint_v3::tests::genesis_initialization_v2_inherits_weights_bit_exact_and_zeros_moments`
    /// for the same in-memory proof at the checkpoint-authority layer.
    #[test]
    fn ladder_init_genesis_bit_equals_the_real_s1_source_checkpoint_weights() {
        let fixture = ladder_init_fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).expect("ladder-init run record");

        let derived_payload = derive_genesis_weights_only_payload_v2_v3(&fixture.reference.payload)
            .expect("weights-only payload derivation must succeed");
        let checkpoint = build_genesis_checkpoint_manifest_v2_v3(
            &run,
            &fixture.reference.checkpoint,
            &derived_payload,
        )
        .expect("GenesisInitializationV2 authoring must succeed for a matching reference");

        let section_len = derived_payload.len() / 3;
        let (weights, moments) = derived_payload.split_at(section_len);
        let (first_moments, second_moments) = moments.split_at(section_len);
        assert_eq!(weights, &fixture.reference.payload[..section_len]);
        assert!(first_moments.iter().all(|byte| *byte == 0));
        assert!(second_moments.iter().all(|byte| *byte == 0));
        assert_eq!(
            checkpoint.model_parameter_sha256(),
            fixture.reference.checkpoint.model_parameter_sha256()
        );

        // Round-trips through decode against the same reference.
        let redecoded = decode_genesis_checkpoint_manifest_v2_v3(
            checkpoint.canonical_bytes(),
            &derived_payload,
            &run,
            &fixture.reference.checkpoint,
        )
        .expect("a self-authored V2 manifest must redecode against the same reference");
        assert_eq!(redecoded.canonical_bytes(), checkpoint.canonical_bytes());
    }

    /// STRUCTURAL OBSTACLE #2, FLIPPED (design directive slice 2). The
    /// previous slice's STOP finding, verbatim: PUBLISHING a V2 genesis
    /// authority to a real Store was independently blocked, because the
    /// publisher re-derives the genesis checkpoint from raw bytes as its
    /// own proof obligation, strictly BEFORE trusting any caller-supplied
    /// `CheckpointManifestV3` value (`decode_generation_candidate_v2`,
    /// `native_training_store_v2.rs`), through the UNCONDITIONAL
    /// `decode_checkpoint_manifest_v3` -- with no reference checkpoint to
    /// validate against and no filesystem access to resolve one (that
    /// module is deliberately I/O-free). This slice closes it: the
    /// publisher's genesis decode now dispatches on the record's own
    /// `contracts.opponent_ladder_initialization` claim
    /// (`decode_genesis_checkpoint_manifest_dispatch_v2_v3`), which for a
    /// ladder-init record validates the candidate's model-parameter digest
    /// against the record's OWN `derived_model_parameter_sha256` field
    /// instead of needing a resolved reference -- so this exact publish now
    /// SUCCEEDS. Proven here through the publisher's own unit surface (the
    /// same one the original STOP finding used), plus an independent
    /// re-validation of the resulting Store through the full walk
    /// (`validate_native_training_store_v2`), so this is not merely "the
    /// publisher returned Ok" but "the walk independently reproves it."
    #[test]
    fn ladder_init_genesis_publishes_through_the_pure_walk_reconciler() {
        let fixture = ladder_init_fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).expect("ladder-init run record");

        let derived_payload = derive_genesis_weights_only_payload_v2_v3(&fixture.reference.payload)
            .expect("weights-only payload derivation must succeed");
        let checkpoint = build_genesis_checkpoint_manifest_v2_v3(
            &run,
            &fixture.reference.checkpoint,
            &derived_payload,
        )
        .expect("GenesisInitializationV2 authoring must succeed for a matching reference");

        let parent = TestParentV1::new("ladder-init-publish-succeeds");
        let bootstrapped = bootstrap_native_training_store_v2(&parent.parent, "store").unwrap();
        let root = bootstrapped.into_root();
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let receipt = publish_genesis_generation_v2(
            &root,
            &run,
            &derived_payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .expect("publishing a V2 genesis must now succeed: the walk no longer needs a reference");
        assert_eq!(receipt.generation_index(), 0);
        assert_eq!(
            receipt.checkpoint_payload_sha256(),
            crate::native_training_store_digest_v1::sha256_v1(&derived_payload)
        );

        // Independently reproves the published generation through the
        // SAME shared walk primitive (`load_generation_v2` /
        // `walk_complete_store_v2`) the original STOP finding named --
        // this is the "walk accepts a V2 genesis" half of the proof.
        let state = validate_native_training_store_v2(&root, &run)
            .expect("the full walk must independently accept the published V2 genesis");
        assert_eq!(state.latest_generation_index(), 0);
        assert_eq!(
            state.latest_checkpoint().model_parameter_sha256(),
            fixture.reference.checkpoint.model_parameter_sha256()
        );
    }

    /// GenesisInitializationV2 END-TO-END SMOKE (design directive slice 2,
    /// deliverable 5): publishes genesis, trains the fixture's one segment
    /// to its target, and fully re-validates the Store -- for a real
    /// ladder-init record initialized from the real S1 gen-32 checkpoint.
    /// This drives the SAME bootstrap/genesis/train/validate sequence
    /// `run_native_science_loop_v1` runs internally (reproduced here
    /// directly, not through that wrapper -- see the STOP note below for
    /// why), proving genesis-publication-through-real-store-machinery AND
    /// training AND store validation all succeed for a continual-init
    /// record, closing the previous slice's STOP finding at the full
    /// science-loop layer, not just the publisher's own unit surface (see
    /// `ladder_init_genesis_publishes_through_the_pure_walk_reconciler`
    /// immediately above for that narrower proof).
    ///
    /// STOP (found while proving this test, reported per this task's own
    /// discipline rather than fixed unilaterally -- out of the minimal
    /// dispatch-site scope this slice named): `run_native_science_loop_v1`'s
    /// own final step -- running the reference (generation 0) and candidate
    /// boundaries through `run_native_checkpoint_v1` for evaluation -- fails
    /// for a continual-init genesis. `run_native_checkpoint_v1` calls
    /// `load_native_checkpoint_inference_v1`
    /// (`native_checkpoint_inference_v1.rs`), whose
    /// `validate_authority_bindings_v1` (same file, around line 582) carries
    /// its OWN independent copy of the "generation 0 implies the run's
    /// common-snapshot digest" invariant:
    /// `if generation == 0 && checkpoint.model_parameter_sha256() !=
    /// snapshot.named_parameter_stream_sha256 { ...AuthorityBinding... }`.
    /// This is a THIRD site carrying the same assumption
    /// `build_genesis_checkpoint_manifest_v3`'s `validate_genesis_snapshot_v3`
    /// and (previously) the walk/publish chokepoint carried -- but it lives
    /// in the CHECKPOINT-INFERENCE layer (evaluation, the checkpoint runner,
    /// ladder-opponent-engine resolution), a different module and a
    /// different invariant than the walk/publish decode dispatch this slice
    /// was scoped to. Extending it is its own surgery (thread a
    /// self-contained-vs-reference distinction into inference loading, or
    /// give `NativeCheckpointInferenceV1` its own dispatch mirroring
    /// `decode_genesis_checkpoint_manifest_dispatch_v2_v3`), not something
    /// to fold in here. This test therefore proves genesis + train +
    /// validate directly (the design directive's own stated fallback:
    /// "genesis-publication-through-real-store-machinery... no training" --
    /// exceeded here, since training succeeds too) and deliberately stops
    /// short of calling `run_native_science_loop_v1` itself, whose last step
    /// would hit this exact obstacle.
    #[test]
    fn ladder_init_science_loop_trains_the_store_end_to_end() {
        let fixture = ladder_init_fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).expect("ladder-init run record");
        let target = run.requested_successful_updates();
        let execution_config = test_execution_config_v2(&run);
        let parent = TestParentV1::new("ladder-init-trains-e2e");

        // Genesis: the same authoring + publish sequence
        // `ladder_init_genesis_publishes_through_the_pure_walk_reconciler`
        // proves in isolation, reused here as the first step of the full
        // loop.
        let bootstrapped = bootstrap_native_training_store_v2(&parent.parent, "store").unwrap();
        let root = bootstrapped.into_root();
        let derived_payload = derive_genesis_weights_only_payload_v2_v3(&fixture.reference.payload)
            .expect("weights-only payload derivation must succeed");
        let checkpoint = build_genesis_checkpoint_manifest_v2_v3(
            &run,
            &fixture.reference.checkpoint,
            &derived_payload,
        )
        .expect("GenesisInitializationV2 authoring must succeed for a matching reference");
        let segment = build_genesis_segment_manifest_v2(&run, &checkpoint).unwrap();
        let boundary =
            build_genesis_native_training_boundary_v2(&run, &segment, &checkpoint).unwrap();
        let reference = build_checkpoint_reference_v2(&run, &boundary).unwrap();
        let latest = build_latest_v2(&boundary, &reference).unwrap();
        let genesis_receipt = publish_genesis_generation_v2(
            &root,
            &run,
            &derived_payload,
            &checkpoint,
            &segment,
            &boundary,
            &reference,
            &latest,
        )
        .expect("publishing a V2 genesis must succeed: the walk no longer needs a reference");
        assert_eq!(genesis_receipt.generation_index(), 0);

        // Train to the exact target -- the identical resume/prepare/publish
        // loop `run_native_science_loop_v1` runs internally.
        let latest_generation_index = loop {
            let resumed =
                resume_native_training_store_v2(&root, &run, execution_config.clone()).unwrap();
            match resumed {
                NativeTrainingStoreResumeV2::Complete {
                    latest_generation_index,
                } => break latest_generation_index,
                NativeTrainingStoreResumeV2::Continue(mut continuation) => {
                    continuation.executor.set_ladder_opponent_v1(None);
                    let prepared = prepare_segment_v2(
                        &mut continuation.executor,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                    )
                    .unwrap();
                    let receipt = crate::native_training_store_v2::publish_prepared_segment_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                    )
                    .unwrap();
                    prepared.commit_v2(receipt).unwrap();
                }
            }
        };
        assert_eq!(latest_generation_index, target);

        let state = validate_native_training_store_v2(&root, &run)
            .expect("the full walk must accept the completed ladder-init run end to end");
        assert_eq!(state.latest_generation_index(), target);

        // The genesis generation specifically still bears the reference's
        // inherited weights (proven via the model-parameter digest, since
        // the genesis boundary itself is no longer latest once training
        // advances past it): reload generation 0 explicitly through the
        // same validated resume machinery every other consumer uses.
        let genesis = load_native_training_boundary_v2(&root, &run, 0)
            .expect("generation 0 must reload through the validated resume machinery");
        assert_eq!(
            genesis.checkpoint().model_parameter_sha256(),
            fixture.reference.checkpoint.model_parameter_sha256()
        );
    }

    /// FLIPS the previous slice's STOP-finding regression (design directive
    /// slice 3): `run_native_science_loop_v1`, called end to end for a
    /// continual-init record on the real S1 gen-32 checkpoint (Sequential
    /// backend, the fixture's small four-update target), no longer fails
    /// CLOSED at `RunFailed` -- the evaluation step's independent
    /// `native_checkpoint_inference_v1.rs::validate_authority_bindings_v1`
    /// generation-0-implies-common-snapshot check now pins generation 0
    /// against the record's own `derived_model_parameter_sha256` when the
    /// init section is present, exactly mirroring the walk/publish dispatch
    /// slice 2 already closed. The wrapper now runs genesis, training, full
    /// Store validation, AND the final reference/candidate evaluation to
    /// completion. Supersedes
    /// `ladder_init_science_loop_wrapper_no_longer_fails_at_genesis_now_fails_at_run`,
    /// which pinned the exact narrower `RunFailed` obstacle this closes.
    #[test]
    fn ladder_init_science_loop_wrapper_completes_genesis_training_and_evaluation_end_to_end() {
        let fixture = ladder_init_fixture_v1();
        let run = decode_train_run_v2(&fixture.run_bytes).expect("ladder-init run record");
        let target = run.requested_successful_updates();
        let (snapshot_manifest, snapshot_payload) = common_model_snapshot_paths_v1();
        let parent = TestParentV1::new("ladder-init-wrapper-e2e");

        let report = run_native_science_loop_v1(
            &parent.parent,
            "store",
            &run,
            test_execution_config_v2(&run),
            &snapshot_manifest,
            &snapshot_payload,
            runner_config_v1(),
            None,
            Some(&fixture.reference),
        )
        .expect(
            "the wrapper must complete genesis, training, and evaluation for a continual-init \
             record",
        );

        assert_eq!(report.latest_generation_index(), target);
        assert_eq!(report.evaluation().reference_generation_index(), 0);
        assert_eq!(report.evaluation().candidate_generation_index(), target);

        // Genesis and training are both durably published, and the Store the
        // wrapper leaves behind independently re-validates end to end.
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(&parent.parent.join("store"))
            .expect("store root must open: genesis and training both committed");
        let state = validate_native_training_store_v2(&root, &run)
            .expect("the Store the wrapper leaves behind is itself fully valid");
        assert_eq!(state.latest_generation_index(), target);

        // Generation 0 specifically still bears the reference's inherited
        // weights, reloaded through the same validated resume machinery the
        // wrapper's own evaluation step used internally.
        let genesis = load_native_training_boundary_v2(&root, &run, 0)
            .expect("generation 0 must reload through the validated resume machinery");
        assert_eq!(
            genesis.checkpoint().model_parameter_sha256(),
            fixture.reference.checkpoint.model_parameter_sha256()
        );
    }
}

#[cfg(test)]
mod live_c2_genesis_tests {
    use super::*;
    use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
    use crate::native_training_executor_v1::run_bound_snapshot_construction_count_scope_v2;
    use crate::native_training_store_run_v2::{
        decode_train_run_v2, test_fixture_bytes_environment_randomization_v2,
        test_fixture_bytes_with_schedule_and_base_seed_wide_environment_v2,
    };
    use std::time::Duration;

    fn run_matched_config_v1(run: &ValidatedTrainRunV2) -> NativeTrainingExecutionConfigV1 {
        let record = run.record();
        let parse_bits = |hex: &str| u32::from_str_radix(hex, 16).unwrap();
        NativeTrainingExecutionConfigV1 {
            run_base_seed: record.schedule.base_seed,
            batch_episodes: run.batch_episodes(),
            deck_ids: record.environment.deck_ids.clone(),
            max_physical_decisions: record.limits.max_physical_decisions,
            max_policy_steps: record.limits.max_policy_steps,
            worker_count: usize::try_from(record.topology.worker_count).unwrap(),
            sessions_per_worker: usize::try_from(record.topology.sessions_per_worker).unwrap(),
            broker_batch_target: usize::try_from(record.topology.broker_batch_target).unwrap(),
            scheduler_timeout: Duration::from_millis(record.topology.scheduler_timeout_ms),
            measure_broker_service_time: record.topology.measure_broker_service_time,
            value_coefficient_bits: parse_bits(&record.optimization.value_coefficient_f32_bits),
            learning_rate_bits: parse_bits(&record.optimization.learning_rate_f32_bits),
            numerical_backend: NativeTrainingNumericalBackendV1::Sequential,
            backward_worker_limit: 1,
        }
    }

    /// Live C2 callsite witness for both ordinary genesis branches: genesis
    /// bytes are mode-free, so only the run-bound construction counters can
    /// prove which constructor ran. A narrow V2 run must construct run-bound
    /// narrow exactly once, a genuinely wide V2 run must construct run-bound
    /// wide exactly once, and each payload equals its raw sibling's payload,
    /// which is exactly the mode-free-bytes fact that makes the counter the
    /// only honest witness. Reverting either branch to a raw constructor
    /// fails the counts.
    #[test]
    fn ordinary_genesis_dispatch_constructs_run_bound_in_both_branches() {
        use crate::common_model_snapshot_v1::{
            common_model_snapshot_paths_v1, wide_model_snapshot_paths_v1,
        };
        use crate::native_training_store_run_v2::NativeRunEnvironmentTrajectoryContractV1;

        let _lock = crate::async_flat_scored_rollout_v1::acquire_async_flat_scored_test_lock_v1();
        let narrow_run =
            decode_train_run_v2(&test_fixture_bytes_environment_randomization_v2()).unwrap();
        let (narrow_manifest, narrow_payload) = common_model_snapshot_paths_v1();
        let scope = run_bound_snapshot_construction_count_scope_v2();
        let payload = ordinary_genesis_payload_run_bound_v2(
            &narrow_run,
            run_matched_config_v1(&narrow_run),
            &narrow_manifest,
            &narrow_payload,
        )
        .expect("the narrow V2 ordinary genesis payload must build");
        assert_eq!(
            scope.counts(),
            (1, 0),
            "narrow genesis constructs run-bound narrow"
        );
        let raw = NativeTrainingExecutorV1::from_common_model_snapshot_v1(
            run_matched_config_v1(&narrow_run),
            &narrow_manifest,
            &narrow_payload,
        )
        .unwrap();
        assert_eq!(
            payload,
            raw.checkpoint_candidate_v1().unwrap().payload().to_vec(),
            "genesis bytes are mode-free"
        );
        drop(scope);

        let wide_run = decode_train_run_v2(
            &test_fixture_bytes_with_schedule_and_base_seed_wide_environment_v2(
                NativeTrainingNumericalBackendV1::Sequential,
                2,
                4,
                4,
                2,
                4,
                8,
                32_768,
                65_536,
                71_501,
            ),
        )
        .unwrap();
        assert_eq!(
            wide_run.environment_trajectory_contract_v1(),
            NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2
        );
        let (wide_manifest, wide_payload) = wide_model_snapshot_paths_v1();
        let scope = run_bound_snapshot_construction_count_scope_v2();
        let payload = ordinary_genesis_payload_run_bound_v2(
            &wide_run,
            run_matched_config_v1(&wide_run),
            &wide_manifest,
            &wide_payload,
        )
        .expect("the wide V2 ordinary genesis payload must build");
        assert_eq!(
            scope.counts(),
            (0, 1),
            "wide genesis constructs run-bound wide"
        );
        let raw_wide = NativeTrainingExecutorV1::from_common_model_snapshot_wide_v1(
            run_matched_config_v1(&wide_run),
            &wide_manifest,
            &wide_payload,
        )
        .unwrap();
        assert_eq!(
            payload,
            raw_wide
                .checkpoint_candidate_v1()
                .unwrap()
                .payload()
                .to_vec(),
            "wide genesis bytes are mode-free"
        );
    }
}
