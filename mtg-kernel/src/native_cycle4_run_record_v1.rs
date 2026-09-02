//! Launcher-level builder for one cycle-4 arm's `run.json`
//! (`docs/native_cycle4_arm_launcher_v1.md` Section 1, under the ratified
//! `OX_CYCLE4_PREREG_SKETCH_V2.md`).
//!
//! Before this module the only cycle-4 run-record shapes in the tree were
//! `#[cfg(test)]` fixtures, so an operator had no way to produce the one
//! input every arm invocation requires. This builds it, deterministically,
//! from exactly three sources and nothing else:
//!
//! 1. the ARM KIND, which decides `population_program_v2_cycle4.arm_kind`,
//!    `static_pool`, the presence of `trainer_v4_candidate`, the top-level
//!    loss identity, and the arm's own formal base seed;
//! 2. the PARENT Store (the cycle-3 lineage run at trainee-local 896, which
//!    is that Store's own generation 896), which supplies the whole device
//!    contract, train step, model architecture, schedule shape, environment,
//!    toolchain and runtime tuple -- everything the cycle-4 validators do
//!    not pin -- plus the six pinned digests of
//!    `contracts.opponent_ladder_initialization`, resolved through the same
//!    `stage_ladder_checkpoint_initialization_v1` the arm bin's own genesis
//!    bootstrap re-derives, so the two can never disagree;
//! 3. the PINNED cycle-4 literals already compiled into
//!    `native_training_store_run_v2` (pre-registration SHA, refresh-manifest
//!    schema, 896/2944/128, BETA bits, cell cap, the v4 contract document
//!    digest, the CUDA-burn-dense backend, 2048 requested successful
//!    updates), imported rather than restated.
//!
//! Nothing else enters. There is no clock, no environment variable, no
//! random source and no operator-chosen field, so two invocations against
//! the same parent Store produce byte-identical output; the bin asserts that
//! property is testable by printing the record's own SHA-256.
//!
//! The three formal base seeds are declared HERE and nowhere else. The
//! pre-registration (Section 8) requires the seed-schedule policy to be
//! explicit and the arms' seed domains to be disjoint; that is what
//! [`cycle4_arm_base_seed_v1`] states, and the reserved-band comment on it
//! is the whole policy.
//!
//! Everything the builder produces is passed through
//! `validate_train_run_record_v2` (which runs `validate_contracts_v2`,
//! `validate_population_program_v2_cycle4` and the whole record-level V2
//! validation) and then through
//! `native_cycle4_arm_v1::validate_cycle4_arm_run_record_v1` (the launcher's
//! own record-level check) BEFORE any bytes are returned. A record this
//! module hands back is one the arm bin has already agreed to accept.

use crate::native_cycle4_arm_v1::{validate_cycle4_arm_run_record_v1, Cycle4ArmKindV1};
use crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_initialization_v1;
use crate::native_policy_baseline_state_v4::NATIVE_BASELINE_STATE_SCHEMA_V4;
use crate::native_policy_train_step_v1::CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1;
use crate::native_store_production_capture_v2::{
    capture_launcher_build_provenance_v2, current_launcher_build_identity_v2,
    decode_launcher_build_identity_v2, LauncherBuildProvenanceV2,
};
use crate::native_training_store_run_v2::{
    decode_train_run_v2, refresh_derived_fields_v2, validate_train_run_record_v2,
    OpponentLadderInitializationContractV1, PopulationProgramContractV2Cycle4, TrainRunContractsV2,
    TrainRunScheduleV2, TrainRunV2, TrainerV4CandidateContractV1, ValidatedTrainRunV2,
    CUDA_RUNTIME_TUPLE_IDENTITY_V2, CYCLE4_ARM_LAUNCHER_BINARY_NAME_V1, CYCLE4_PREREG_SHA256_V1,
    CYCLE4_REFRESH_INTERVAL_V1, CYCLE4_REFRESH_MANIFEST_SCHEMA_V1,
    CYCLE4_TOTAL_SUCCESSFUL_UPDATES_V1, CYCLE4_TRAINEE_START_GENERATION_V1,
    CYCLE4_TRAINEE_STOP_GENERATION_V1, CYCLE4_TRAINER_V4_BETA_F32_BITS_V1,
    CYCLE4_TRAINER_V4_CELL_CAP_V1, CYCLE4_TRAINER_V4_CONTRACT_DOCUMENT_SHA256_V1,
    CYCLE4_TRAINER_V4_LOSS_IDENTITY_V1, CYCLE4_TRAINER_V4_NUMERICAL_BACKEND_V1,
    NATIVE_TRAINING_STORE_IDENTITY_V2, TRAIN_RUN_SCHEMA_V2,
};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The arm's formal training base seed.
///
/// The literals and the disjoint-domain policy live on
/// [`Cycle4ArmKindV1::formal_base_seed_v1`], not here: the mapping is a
/// property of the ARM, and `validate_run_contract_v1` enforces it on every
/// invocation, including one handed a record this builder never wrote. This
/// is the builder's view of the same fact.
#[must_use]
pub const fn cycle4_arm_base_seed_v1(arm: Cycle4ArmKindV1) -> u64 {
    arm.formal_base_seed_v1()
}

/// One run-record build request. Every field is required; there are no
/// defaults and no environment lookups.
#[derive(Clone, Debug)]
pub struct Cycle4RunRecordRequestV1 {
    pub arm: Cycle4ArmKindV1,
    /// The cycle-3 lineage Store the arm's genesis weights come from.
    pub parent_store_root: PathBuf,
    /// The parent's generation in ITS OWN Store numbering, which for the
    /// cycle-3 focal run is also the trainee-local number: 896. Any other
    /// value is rejected before the parent is even staged: cycle 4 starts at
    /// the pre-registered g896 prefix and nowhere else.
    pub parent_generation: u64,
    /// The cycle-4 arm launcher that will publish the Store
    /// (`cycle4_arm_v1.exe`). Its build provenance, not the parent record's,
    /// is what the assembled record declares, so the record describes the
    /// executable that produced it.
    pub arm_executable: PathBuf,
}

/// What one build produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cycle4RunRecordOutcomeV1 {
    pub arm_kind: String,
    pub base_seed: u64,
    /// SHA-256 of `canonical_bytes`, which is the arm's `run_sha256` and the
    /// identity every manifest, locator and origin record binds.
    pub run_sha256: String,
    pub canonical_bytes: Vec<u8>,
    /// The parent record's own `run_sha256`, as resolved from the parent
    /// Store, restated so a caller can cross-check it without re-reading.
    pub parent_run_sha256: String,
    pub parent_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cycle4RunRecordErrorV1 {
    /// The parent Store's `run.json` could not be read.
    ParentRunFileIo { path: PathBuf, detail: String },
    /// The parent Store's `run.json` failed V2 record validation.
    ParentRunRejected { detail: String },
    /// `--parent-generation` named a generation cycle 4 does not start from.
    ParentGenerationNotPinned { requested: u64, required: u64 },
    /// The pinned parent checkpoint at `--parent-generation` could not be
    /// resolved from the parent Store.
    ParentCheckpointRejected { detail: String },
    /// The arm launcher executable could not be captured.
    ArmExecutableRejected { path: PathBuf, detail: String },
    /// The arm launcher did not report this build's own identity, so the two
    /// binaries come from different builds and must not co-author a record.
    ArmBuildIdentityMismatch { path: PathBuf, detail: String },
    /// The parent record does not carry the ladder tuple a cycle-4 arm needs
    /// (`opponent_ladder_pool` plus `opponent_schedule_v2` under the ladder
    /// opponent identity); the arm record is built FROM it, so it cannot be
    /// synthesized here.
    ParentMissingLadderTuple { section: &'static str },
    /// The assembled record failed `validate_train_run_record_v2`.
    RecordRejected { detail: String },
    /// The assembled record failed the launcher's own record-level check.
    ArmContractRejected { code: String, detail: String },
}

impl Display for Cycle4RunRecordErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParentRunFileIo { path, detail } => {
                write!(formatter, "parent run.json unreadable ({}): {detail}", path.display())
            }
            Self::ParentRunRejected { detail } => {
                write!(formatter, "parent run.json rejected: {detail}")
            }
            Self::ParentGenerationNotPinned {
                requested,
                required,
            } => write!(
                formatter,
                "parent generation {requested} is not the pre-registered cycle-4 start {required}; a cycle-4 arm is seeded from the cycle-3 focal run's g{required} prefix and from no other generation"
            ),
            Self::ParentCheckpointRejected { detail } => {
                write!(formatter, "parent checkpoint rejected: {detail}")
            }
            Self::ArmExecutableRejected { path, detail } => write!(
                formatter,
                "the arm launcher executable could not be captured ({}): {detail}",
                path.display()
            ),
            Self::ArmBuildIdentityMismatch { path, detail } => write!(
                formatter,
                "the arm launcher ({}) does not report this builder's own build identity: {detail}; a record must not be built by one build and published by another",
                path.display()
            ),
            Self::ParentMissingLadderTuple { section } => write!(
                formatter,
                "the parent run record carries no contracts.{section}; a cycle-4 arm record cannot be built from it"
            ),
            Self::RecordRejected { detail } => {
                write!(formatter, "assembled run record rejected: {detail}")
            }
            Self::ArmContractRejected { code, detail } => {
                write!(formatter, "assembled run record rejected by the arm launcher ({code}): {detail}")
            }
        }
    }
}

impl Error for Cycle4RunRecordErrorV1 {}

type Result<T> = std::result::Result<T, Cycle4RunRecordErrorV1>;

/// The `trainer_v4_candidate` section, wholly from the pinned literals.
fn trainer_v4_candidate_section_v1() -> TrainerV4CandidateContractV1 {
    TrainerV4CandidateContractV1 {
        loss_identity: CYCLE4_TRAINER_V4_LOSS_IDENTITY_V1.to_owned(),
        baseline_schema: NATIVE_BASELINE_STATE_SCHEMA_V4.to_owned(),
        beta_f32_bits: CYCLE4_TRAINER_V4_BETA_F32_BITS_V1.to_owned(),
        cell_cap: CYCLE4_TRAINER_V4_CELL_CAP_V1,
        contract_document_sha256: CYCLE4_TRAINER_V4_CONTRACT_DOCUMENT_SHA256_V1.to_owned(),
        numerical_backend: CYCLE4_TRAINER_V4_NUMERICAL_BACKEND_V1.to_owned(),
    }
}

/// The `population_program_v2_cycle4` section, wholly from the pinned
/// literals plus the arm kind.
fn population_program_section_v1(arm: Cycle4ArmKindV1) -> PopulationProgramContractV2Cycle4 {
    PopulationProgramContractV2Cycle4 {
        prereg_sha256: CYCLE4_PREREG_SHA256_V1.to_owned(),
        refresh_manifest_schema: CYCLE4_REFRESH_MANIFEST_SCHEMA_V1.to_owned(),
        arm_kind: arm.wire_v1().to_owned(),
        trainee_start_generation: CYCLE4_TRAINEE_START_GENERATION_V1,
        trainee_stop_generation: CYCLE4_TRAINEE_STOP_GENERATION_V1,
        refresh_interval: CYCLE4_REFRESH_INTERVAL_V1,
        static_pool: arm.static_pool_v1(),
    }
}

/// Requires the arm launcher to report exactly THIS builder's own embedded
/// build identity.
///
/// The record carries this build's package, toolchain and source tree beside
/// the arm launcher's executable hash. Those two halves are only coherent if
/// both binaries came from one build, and nothing in the file system says so:
/// a hash is just a hash. So the launcher is asked, by running it with
/// `--print-build-identity`, which reads nothing, touches no device, and
/// writes its embedded tuple as canonical JSON.
///
/// # Errors
///
/// Returns [`Cycle4RunRecordErrorV1::ArmBuildIdentityMismatch`] if the child
/// cannot be run, exits nonzero, prints something that is not a canonical
/// identity, or reports an identity that is not this build's.
fn require_arm_executable_build_identity_v1(arm_executable: &Path) -> Result<()> {
    let reject = |detail: String| Cycle4RunRecordErrorV1::ArmBuildIdentityMismatch {
        path: arm_executable.to_path_buf(),
        detail,
    };
    let output = Command::new(arm_executable)
        .arg("--print-build-identity")
        .output()
        .map_err(|error| reject(format!("--print-build-identity could not run: {error}")))?;
    if !output.status.success() {
        return Err(reject(format!(
            "--print-build-identity exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    require_reported_identity_is_own_v1(arm_executable, &output.stdout)
}

/// The decision half of [`require_arm_executable_build_identity_v1`], over
/// the bytes the launcher printed. Split out so the comparison is testable
/// without a second binary on disk.
fn require_reported_identity_is_own_v1(arm_executable: &Path, stdout: &[u8]) -> Result<()> {
    let reject = |detail: String| Cycle4RunRecordErrorV1::ArmBuildIdentityMismatch {
        path: arm_executable.to_path_buf(),
        detail,
    };
    // The canonical encoding ENDS with a single LF, so the framing is
    // normalized rather than trimmed: trailing whitespace (a console CRLF,
    // say) is removed and exactly one LF restored. Trimming outright would
    // strip the terminator that makes the bytes canonical.
    let printed = String::from_utf8_lossy(stdout);
    let body = printed.trim_end_matches(['\r', '\n', ' ', '\t']);
    let reported = decode_launcher_build_identity_v2(format!("{body}\n").as_bytes())
        .map_err(|error| reject(error.to_string()))?;
    let own = current_launcher_build_identity_v2();
    if reported != own {
        return Err(reject(format!(
            "the launcher reports commit {} with features [{}]; this builder is commit {} with features [{}]",
            reported.source_git_commit,
            reported.enabled_features.join(","),
            own.source_git_commit,
            own.enabled_features.join(",")
        )));
    }
    Ok(())
}

/// Builds, validates and returns one arm's canonical `run.json` bytes.
///
/// # Errors
///
/// Returns a [`Cycle4RunRecordErrorV1`] if the parent Store cannot be read
/// or resolved, if the parent record lacks the ladder tuple, or if the
/// assembled record is rejected by either the V2 record validator or the
/// cycle-4 launcher's own record-level check. Nothing is written by this
/// function; publication is the caller's.
pub fn build_cycle4_arm_run_record_v1(
    request: &Cycle4RunRecordRequestV1,
) -> Result<Cycle4RunRecordOutcomeV1> {
    // The pinned start generation is checked BEFORE anything is staged: a
    // request naming the lineage tip must not reach
    // `stage_ladder_checkpoint_initialization_v1`, because whatever that
    // staged would be written into `opponent_ladder_initialization` and the
    // arm would then be seeded from it. The wrapper performs the same
    // comparison against the finished record as a second, independent gate.
    if request.parent_generation != CYCLE4_TRAINEE_START_GENERATION_V1 {
        return Err(Cycle4RunRecordErrorV1::ParentGenerationNotPinned {
            requested: request.parent_generation,
            required: CYCLE4_TRAINEE_START_GENERATION_V1,
        });
    }

    let parent_root: &Path = request.parent_store_root.as_path();
    let parent_run_path = parent_root.join("run.json");
    let parent_bytes = std::fs::read(&parent_run_path).map_err(|error| {
        Cycle4RunRecordErrorV1::ParentRunFileIo {
            path: parent_run_path.clone(),
            detail: error.to_string(),
        }
    })?;
    let parent = decode_train_run_v2(&parent_bytes).map_err(|error| {
        Cycle4RunRecordErrorV1::ParentRunRejected {
            detail: error.to_string(),
        }
    })?;

    // The pinned genesis origin, resolved from the parent Store's own files
    // through the SAME helper the arm bin's genesis bootstrap re-derives and
    // compares against. Nothing here is operator-supplied.
    let initialization =
        stage_ladder_checkpoint_initialization_v1(parent_root, request.parent_generation).map_err(
            |error| Cycle4RunRecordErrorV1::ParentCheckpointRejected {
                detail: format!(
                    "{} generation {}: {error}",
                    parent_root.display(),
                    request.parent_generation
                ),
            },
        )?;
    let parent_run_sha256 = initialization.source_run_sha256.clone();

    // The provenance the record declares is THIS build's and the arm
    // launcher's, never the parent's. Inheriting the parent's package,
    // toolchain, source and runtime would make a cycle-4 record describe an
    // older executable built from an older tree, possibly without the CUDA
    // feature the arms train under.
    require_arm_executable_build_identity_v1(request.arm_executable.as_path())?;
    let provenance = capture_launcher_build_provenance_v2(
        request.arm_executable.as_path(),
        CYCLE4_ARM_LAUNCHER_BINARY_NAME_V1,
        CUDA_RUNTIME_TUPLE_IDENTITY_V2,
        CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1,
    )
    .map_err(|error| Cycle4RunRecordErrorV1::ArmExecutableRejected {
        path: request.arm_executable.clone(),
        detail: error.to_string(),
    })?;

    let validated =
        assemble_cycle4_arm_run_record_v1(request.arm, &parent, initialization, provenance)?;
    Ok(Cycle4RunRecordOutcomeV1 {
        arm_kind: request.arm.wire_v1().to_owned(),
        base_seed: cycle4_arm_base_seed_v1(request.arm),
        run_sha256: validated.run_sha256().to_owned(),
        canonical_bytes: validated.canonical_bytes().to_vec(),
        parent_run_sha256,
        parent_generation: request.parent_generation,
    })
}

/// The pure half of the build: everything from an already-decoded parent
/// record and an already-resolved genesis origin to a validated cycle-4 arm
/// record. Split out from [`build_cycle4_arm_run_record_v1`] so the
/// assembly rules are testable without a Store on disk, and so the I/O half
/// stays a thin read-and-resolve.
fn assemble_cycle4_arm_run_record_v1(
    arm: Cycle4ArmKindV1,
    parent: &ValidatedTrainRunV2,
    initialization: OpponentLadderInitializationContractV1,
    provenance: LauncherBuildProvenanceV2,
) -> Result<ValidatedTrainRunV2> {
    assemble_with_base_seed_v1(
        arm,
        parent,
        initialization,
        provenance,
        cycle4_arm_base_seed_v1(arm),
    )
}

/// The assembly body, with the base seed as an explicit parameter.
///
/// Production has exactly one caller, above, which passes the arm's pinned
/// seed. The parameter exists so a test can present a record carrying the
/// WRONG arm's seed and prove the launcher's own record-level validator
/// refuses it -- the seed rule has to hold for records this builder never
/// wrote, so it cannot be tested by only ever building correct ones.
fn assemble_with_base_seed_v1(
    arm: Cycle4ArmKindV1,
    parent: &ValidatedTrainRunV2,
    initialization: OpponentLadderInitializationContractV1,
    provenance: LauncherBuildProvenanceV2,
    base_seed: u64,
) -> Result<ValidatedTrainRunV2> {
    // Restated here as well as in the caller: this function is what a test
    // drives, and a record whose pinned origin is not g896 must not be
    // assemblable by any path.
    if initialization.generation != CYCLE4_TRAINEE_START_GENERATION_V1 {
        return Err(Cycle4RunRecordErrorV1::ParentGenerationNotPinned {
            requested: initialization.generation,
            required: CYCLE4_TRAINEE_START_GENERATION_V1,
        });
    }
    let parent_record = parent.record();
    let parent_contracts = &parent_record.contracts;
    let ladder_pool = parent_contracts.opponent_ladder_pool.clone().ok_or(
        Cycle4RunRecordErrorV1::ParentMissingLadderTuple {
            section: "opponent_ladder_pool",
        },
    )?;
    let opponent_schedule = parent_contracts.opponent_schedule_v2.clone().ok_or(
        Cycle4RunRecordErrorV1::ParentMissingLadderTuple {
            section: "opponent_schedule_v2",
        },
    )?;

    let uses_v4 = arm.uses_baseline_v4_v1();
    let mut loss = parent_contracts.loss.clone();
    if uses_v4 {
        loss.identity = CYCLE4_TRAINER_V4_LOSS_IDENTITY_V1.to_owned();
    }
    // Set, not inherited, and for every arm: all three train on the CUDA
    // burn-dense backend, `validate_cross_bindings_v2` binds this field to
    // `runtime.numerical_backend_identity` (which the captured provenance
    // above pins to the CUDA tuple), and the v4 arms admit no other backend.
    let mut train_step = parent_contracts.train_step.clone();
    train_step.numerical_backend_identity =
        CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1.to_owned();

    let contracts = TrainRunContractsV2 {
        trainer_identity: parent_contracts.trainer_identity.clone(),
        identity_bundle_identity: parent_contracts.identity_bundle_identity.clone(),
        // Placeholder: `refresh_derived_fields_v2` below recomputes it. The
        // parent's value would be wrong for this record and must never
        // survive into it.
        identity_bundle_sha256: String::new(),
        tensorizer: parent_contracts.tensorizer.clone(),
        model: parent_contracts.model.clone(),
        loss,
        train_step,
        optimizer: parent_contracts.optimizer.clone(),
        trainer_schedule: parent_contracts.trainer_schedule.clone(),
        learner_sampler: parent_contracts.learner_sampler.clone(),
        opponent_policy: parent_contracts.opponent_policy.clone(),
        opponent_sampler: parent_contracts.opponent_sampler.clone(),
        opponent_ladder_pool: Some(ladder_pool),
        opponent_ladder_initialization: Some(initialization),
        opponent_schedule_v2: Some(opponent_schedule),
        trajectory: parent_contracts.trajectory.clone(),
        // Also fully recomputed below (core AND digest); cloned only so the
        // struct is complete before that recomputation runs.
        standalone_semantics: parent_contracts.standalone_semantics.clone(),
        // Every predecessor program section is DROPPED, not carried: a
        // cycle-4 arm runs the population engine only, and
        // `validate_population_program_v2_cycle4` rejects a record that
        // carries any of them alongside its own section.
        wide_model_experiment_v1: None,
        population_program_v1: None,
        response_exploiter_v1: None,
        population_program_v2_cycle2: None,
        population_program_v2_cycle3: None,
        trainer_v4_candidate: uses_v4.then(trainer_v4_candidate_section_v1),
        population_program_v2_cycle4: Some(population_program_section_v1(arm)),
    };

    // The schedule SHAPE is the parent's (batch episodes, checkpoint
    // segment, seat and pair-seed rules, measurement mode); only the two
    // fields cycle-4 owns are replaced.
    let schedule = TrainRunScheduleV2 {
        base_seed,
        requested_successful_updates: CYCLE4_TOTAL_SUCCESSFUL_UPDATES_V1,
        ..parent_record.schedule.clone()
    };

    let LauncherBuildProvenanceV2 {
        package,
        toolchain,
        source,
        runtime,
    } = provenance;
    let mut record = TrainRunV2 {
        schema: TRAIN_RUN_SCHEMA_V2.to_owned(),
        store_identity: NATIVE_TRAINING_STORE_IDENTITY_V2.to_owned(),
        // Provenance is the CURRENT build's and the arm launcher's. The
        // environment below stays the parent's: it is a catalog fact, not a
        // build fact, and `validate_environment_v2` plus
        // `classify_catalog_profile_v1` check it against the live catalog at
        // decode, so an inherited environment that disagreed with this build
        // would not decode at all.
        package,
        toolchain,
        source,
        runtime,
        environment: parent_record.environment.clone(),
        contracts,
        model_snapshot: parent_record.model_snapshot.clone(),
        optimization: parent_record.optimization.clone(),
        schedule,
        limits: parent_record.limits.clone(),
        topology: parent_record.topology.clone(),
        artifact_schemas: parent_record.artifact_schemas.clone(),
        publication: parent_record.publication.clone(),
        nonclaims: parent_record.nonclaims.clone(),
    };
    refresh_derived_fields_v2(&mut record).map_err(|error| {
        Cycle4RunRecordErrorV1::RecordRejected {
            detail: error.to_string(),
        }
    })?;

    let validated = validate_train_run_record_v2(record).map_err(|error| {
        Cycle4RunRecordErrorV1::RecordRejected {
            detail: error.to_string(),
        }
    })?;
    validate_cycle4_arm_run_record_v1(&validated, arm).map_err(|error| {
        Cycle4RunRecordErrorV1::ArmContractRejected {
            code: error.code_v1().to_owned(),
            detail: error.to_string(),
        }
    })?;

    Ok(validated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
    use crate::native_store_production_capture_v2::{
        current_launcher_build_identity_json_v2, require_run_record_matches_provenance_v2,
        test_launcher_build_provenance_v2,
    };
    use crate::native_training_store_run_v2::{
        test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2,
        test_fixture_ladder_initialization_v1, test_fixture_ladder_pool_v2,
    };

    /// The pinned genesis origin at the pre-registered start generation.
    /// The shared fixture pins an older program's parent generation, and a
    /// cycle-4 record may only ever be seeded from g896.
    fn pinned_origin_v1() -> OpponentLadderInitializationContractV1 {
        let mut initialization = test_fixture_ladder_initialization_v1();
        initialization.generation = CYCLE4_TRAINEE_START_GENERATION_V1;
        initialization
    }

    fn provenance_v1() -> LauncherBuildProvenanceV2 {
        test_launcher_build_provenance_v2(
            CYCLE4_ARM_LAUNCHER_BINARY_NAME_V1,
            CUDA_RUNTIME_TUPLE_IDENTITY_V2,
            CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1,
        )
    }

    const ARMS_V1: [Cycle4ArmKindV1; 3] = [
        Cycle4ArmKindV1::ControlR,
        Cycle4ArmKindV1::StaticRb,
        Cycle4ArmKindV1::TreatmentRb,
    ];

    /// A parent record shaped like the real cycle-3 lineage run: the ladder
    /// tuple (pool, schedule, pinned initialization), the CUDA-burn-dense
    /// backend the arms inherit, and the same 64/4 schedule shape.
    fn parent_v1() -> ValidatedTrainRunV2 {
        let bytes = test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2(
            NativeTrainingNumericalBackendV1::CudaBurnDense,
            64,
            4,
            2_048,
            2,
            32,
            16,
            1_024,
            2_048,
            977_002,
            test_fixture_ladder_pool_v2(),
            test_fixture_ladder_initialization_v1(),
        );
        decode_train_run_v2(&bytes).expect("the parent fixture must validate")
    }

    fn build_v1(arm: Cycle4ArmKindV1) -> ValidatedTrainRunV2 {
        assemble_cycle4_arm_run_record_v1(arm, &parent_v1(), pinned_origin_v1(), provenance_v1())
            .expect("the assembled cycle-4 arm record must validate")
    }

    /// A parent generation other than the pre-registered 896 is refused
    /// before anything is staged from it, so `-GenesisParentGeneration 2048`
    /// can never seed cycle 4 from the cycle-3 lineage tip.
    #[test]
    fn only_the_pinned_parent_generation_is_admissible_v1() {
        for generation in [0_u64, 384, 895, 897, 2048] {
            let mut initialization = pinned_origin_v1();
            initialization.generation = generation;
            let error = assemble_cycle4_arm_run_record_v1(
                Cycle4ArmKindV1::TreatmentRb,
                &parent_v1(),
                initialization,
                provenance_v1(),
            )
            .expect_err("a parent generation other than 896 must be refused");
            assert_eq!(
                error,
                Cycle4RunRecordErrorV1::ParentGenerationNotPinned {
                    requested: generation,
                    required: CYCLE4_TRAINEE_START_GENERATION_V1,
                }
            );
        }
        // And 896 itself still builds.
        assert_eq!(
            build_v1(Cycle4ArmKindV1::TreatmentRb)
                .record()
                .contracts()
                .opponent_ladder_initialization
                .as_ref()
                .expect("pinned origin")
                .generation,
            CYCLE4_TRAINEE_START_GENERATION_V1
        );
    }

    /// The record's provenance is THIS build's and the arm launcher's, not
    /// the parent's: a cycle-4 record must not describe an older executable
    /// built from an older tree, possibly without the CUDA feature.
    #[test]
    fn provenance_is_the_current_build_not_the_parents_v1() {
        let parent = parent_v1();
        let built = assemble_cycle4_arm_run_record_v1(
            Cycle4ArmKindV1::TreatmentRb,
            &parent,
            pinned_origin_v1(),
            provenance_v1(),
        )
        .expect("the assembled record must validate");
        let expected = provenance_v1();
        let record = built.record();

        assert_eq!(record.package, expected.package);
        assert_eq!(record.toolchain, expected.toolchain);
        assert_eq!(record.source, expected.source);
        assert_eq!(record.runtime, expected.runtime);

        let parent_record = parent.record();
        assert_ne!(record.package, parent_record.package);
        assert_ne!(record.source, parent_record.source);
        // The launcher the record names is the one that publishes the Store.
        assert_eq!(
            record.source.binary_name,
            CYCLE4_ARM_LAUNCHER_BINARY_NAME_V1
        );
        // And the device contract is the CUDA pair, for every arm kind.
        assert_eq!(
            record.runtime.tuple_identity,
            CUDA_RUNTIME_TUPLE_IDENTITY_V2
        );
        assert_eq!(
            record.runtime.numerical_backend_identity,
            CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1
        );
        assert_eq!(
            record.contracts().train_step.numerical_backend_identity,
            CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1
        );
    }

    /// The arm-to-base-seed mapping is enforced by the LAUNCHER's own
    /// record-level validator, not only by the builder, so a record
    /// carrying another arm's seed is refused even though it is otherwise a
    /// perfectly valid cycle-4 record for its declared arm kind.
    #[test]
    fn the_launcher_rejects_a_record_carrying_another_arms_seed_v1() {
        for arm in ARMS_V1 {
            for other in ARMS_V1 {
                if other == arm {
                    continue;
                }
                let wrong_seed = other.formal_base_seed_v1();
                let error = assemble_with_base_seed_v1(
                    arm,
                    &parent_v1(),
                    pinned_origin_v1(),
                    provenance_v1(),
                    wrong_seed,
                )
                .expect_err("another arm's base seed must be refused");
                assert_eq!(
                    error,
                    Cycle4RunRecordErrorV1::ArmContractRejected {
                        code: "cycle4_arm_v1_base_seed_mismatch".to_owned(),
                        detail: format!(
                            "cycle4_arm_v1_base_seed_mismatch: {} trains under base seed {}, but the run record declares {wrong_seed}",
                            arm.wire_v1(),
                            arm.formal_base_seed_v1()
                        ),
                    }
                );
            }
        }
    }

    #[test]
    fn every_arm_kind_assembles_a_record_that_validates_v1() {
        for arm in ARMS_V1 {
            let built = build_v1(arm);
            let program = built
                .record()
                .contracts()
                .population_program_v2_cycle4
                .as_ref()
                .expect("the cycle-4 section is required");
            assert_eq!(program.arm_kind, arm.wire_v1());
            assert_eq!(program.static_pool, arm.static_pool_v1());
            assert_eq!(program.prereg_sha256, CYCLE4_PREREG_SHA256_V1);
            assert_eq!(
                program.trainee_start_generation,
                CYCLE4_TRAINEE_START_GENERATION_V1
            );
            assert_eq!(
                program.trainee_stop_generation,
                CYCLE4_TRAINEE_STOP_GENERATION_V1
            );
            assert_eq!(program.refresh_interval, CYCLE4_REFRESH_INTERVAL_V1);
            assert_eq!(
                built.requested_successful_updates(),
                CYCLE4_TOTAL_SUCCESSFUL_UPDATES_V1
            );
            assert_eq!(
                built.record().schedule().base_seed,
                cycle4_arm_base_seed_v1(arm)
            );
            // The pinned genesis origin is bound.
            assert!(built
                .record()
                .contracts()
                .opponent_ladder_initialization
                .is_some());
            // Re-decoding the exact bytes reproduces the same identity, which
            // is what the arm bin does with the published file.
            let decoded = decode_train_run_v2(built.canonical_bytes())
                .expect("the published bytes must decode");
            assert_eq!(decoded.run_sha256(), built.run_sha256());
        }
    }

    /// CONTROL-R carries no `trainer_v4_candidate` and keeps the v3 loss;
    /// both rb arms carry it and swap the loss identity.
    #[test]
    fn the_v4_section_is_present_exactly_on_the_rb_arms_v1() {
        let control = build_v1(Cycle4ArmKindV1::ControlR);
        assert!(control.record().contracts().trainer_v4_candidate.is_none());
        assert_ne!(
            control.record().contracts().loss.identity,
            CYCLE4_TRAINER_V4_LOSS_IDENTITY_V1
        );
        for arm in [Cycle4ArmKindV1::StaticRb, Cycle4ArmKindV1::TreatmentRb] {
            let built = build_v1(arm);
            let trainer = built
                .record()
                .contracts()
                .trainer_v4_candidate
                .as_ref()
                .expect("an rb arm requires trainer_v4_candidate");
            assert_eq!(trainer.beta_f32_bits, CYCLE4_TRAINER_V4_BETA_F32_BITS_V1);
            assert_eq!(trainer.cell_cap, CYCLE4_TRAINER_V4_CELL_CAP_V1);
            assert_eq!(
                trainer.contract_document_sha256,
                CYCLE4_TRAINER_V4_CONTRACT_DOCUMENT_SHA256_V1
            );
            assert_eq!(
                trainer.numerical_backend,
                CYCLE4_TRAINER_V4_NUMERICAL_BACKEND_V1
            );
            assert_eq!(
                built.record().contracts().loss.identity,
                CYCLE4_TRAINER_V4_LOSS_IDENTITY_V1
            );
        }
    }

    /// The three arms' base seeds are pairwise distinct, are the launcher's
    /// own literals, and sit outside the payoff-panel seed band.
    #[test]
    fn arm_base_seed_domains_are_disjoint_v1() {
        let seeds: Vec<u64> = ARMS_V1
            .iter()
            .map(|arm| cycle4_arm_base_seed_v1(*arm))
            .collect();
        let mut sorted = seeds.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seeds.len(), "arm base seeds must be distinct");
        for seed in seeds {
            assert!(
                seed < 4_100_000_000,
                "training seeds must stay below the payoff-panel band"
            );
        }
    }

    /// Every field the cycle-4 pins own is fail-closed: tampering with one
    /// after assembly makes the record undecodable, so no edited copy of a
    /// published run.json can be passed off as this record.
    #[test]
    fn a_tampered_field_is_rejected_v1() {
        let built = build_v1(Cycle4ArmKindV1::TreatmentRb);
        let text = String::from_utf8(built.canonical_bytes().to_vec()).expect("utf-8 record");

        // 1. The arm kind: treatment-rb rewritten to control-r, which the
        //    arm-kind consistency rule forbids beside trainer_v4_candidate.
        let arm_swapped = text.replace(r#""arm_kind":"treatment-rb""#, r#""arm_kind":"control-r""#);
        assert_ne!(
            arm_swapped, text,
            "the tamper must actually change the bytes"
        );
        assert!(decode_train_run_v2(arm_swapped.as_bytes()).is_err());

        // 2. The pre-registration digest.
        let prereg_swapped = text.replace(
            CYCLE4_PREREG_SHA256_V1,
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert_ne!(prereg_swapped, text);
        assert!(decode_train_run_v2(prereg_swapped.as_bytes()).is_err());

        // 3. The program span: a short stop generation.
        let span_swapped = text.replace(
            r#""trainee_stop_generation":2944"#,
            r#""trainee_stop_generation":2048"#,
        );
        assert_ne!(span_swapped, text);
        assert!(decode_train_run_v2(span_swapped.as_bytes()).is_err());

        // 4. The base seed, which the derived digests bind.
        let seed_swapped = text.replace(
            &format!(
                r#""base_seed":{}"#,
                cycle4_arm_base_seed_v1(Cycle4ArmKindV1::TreatmentRb)
            ),
            r#""base_seed":123456"#,
        );
        assert_ne!(seed_swapped, text);
        assert!(decode_train_run_v2(seed_swapped.as_bytes()).is_err());
    }

    /// Two assemblies of the same arm against the same parent produce
    /// byte-identical records: nothing time-, host- or order-dependent
    /// enters, so an operator can rebuild a lost run.json and get the same
    /// campaign identity back.
    #[test]
    fn two_invocations_are_byte_identical_v1() {
        for arm in ARMS_V1 {
            let first = build_v1(arm);
            let second = build_v1(arm);
            assert_eq!(first.canonical_bytes(), second.canonical_bytes());
            assert_eq!(first.run_sha256(), second.run_sha256());
        }
        // ... and the three arms are nonetheless three different runs.
        let identities: Vec<String> = ARMS_V1
            .iter()
            .map(|arm| build_v1(*arm).run_sha256().to_owned())
            .collect();
        let mut sorted = identities.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), identities.len());
    }

    /// An arm whose own build is not the record's build refuses to launch.
    ///
    /// This drives the exact comparison
    /// `require_run_record_is_this_build_v1` performs at bootstrap and at
    /// every interval, with the arm's captured tuple standing in for the
    /// running process's. A record assembled from provenance A must be
    /// refused against a capture that differs in the source commit, and
    /// again against one that differs only in the executable hash.
    #[test]
    fn an_arm_from_a_different_build_refuses_the_record_v1() {
        let built = build_v1(Cycle4ArmKindV1::TreatmentRb);
        // The matching capture is accepted; this is the launch that proceeds.
        require_run_record_matches_provenance_v2(&built, &provenance_v1())
            .expect("the record's own build must be accepted");

        // A different source commit.
        let mut other_commit = provenance_v1();
        other_commit.source.git_commit = "1111111111111111111111111111111111111111".to_owned();
        let error = require_run_record_matches_provenance_v2(&built, &other_commit)
            .expect_err("a record from another commit must be refused");
        assert_eq!(error.code(), "run_record_source_is_not_this_build");

        // The same commit, a different executable.
        let mut other_binary = provenance_v1();
        other_binary.source.binary_sha256 =
            "0000000000000000000000000000000000000000000000000000000000000000".to_owned();
        let error = require_run_record_matches_provenance_v2(&built, &other_binary)
            .expect_err("a record naming another binary must be refused");
        assert_eq!(error.code(), "run_record_source_is_not_this_build");

        // And a different feature set, which lives in `package`.
        let mut other_features = provenance_v1();
        other_features.package.enabled_features =
            vec!["native-training-store-v2-production".to_owned()];
        let error = require_run_record_matches_provenance_v2(&built, &other_features)
            .expect_err("a record from another feature set must be refused");
        assert_eq!(error.code(), "run_record_package_is_not_this_build");
    }

    /// The builder refuses an arm launcher that reports a different build
    /// identity, and accepts only its own.
    #[test]
    fn the_builder_refuses_an_arm_from_a_different_build_v1() {
        let path = Path::new("D:\\release\\cycle4_arm_v1.exe");
        let own =
            current_launcher_build_identity_json_v2().expect("this build reports an identity");

        // Exactly what the launcher prints, and the same bytes reframed the
        // way a console would: both must be accepted.
        require_reported_identity_is_own_v1(path, own.as_bytes())
            .expect("the builder's own identity must be accepted");
        require_reported_identity_is_own_v1(path, own.replace('\n', "\r\n").as_bytes())
            .expect("a CRLF-reframed identity is still this build's");

        // A different commit.
        let own_commit = current_launcher_build_identity_v2().source_git_commit;
        let other = own.replace(&own_commit, "1111111111111111111111111111111111111111");
        assert_ne!(other, own);
        let error = require_reported_identity_is_own_v1(path, other.as_bytes())
            .expect_err("another commit must be refused");
        assert!(matches!(
            error,
            Cycle4RunRecordErrorV1::ArmBuildIdentityMismatch { .. }
        ));

        // A different feature set.
        let dropped = own.replace("\"experimental-burn-net8-packed-cuda-v1\",", "");
        assert_ne!(dropped, own);
        assert!(require_reported_identity_is_own_v1(path, dropped.as_bytes()).is_err());

        // Anything that is not a canonical identity at all.
        assert!(require_reported_identity_is_own_v1(path, b"").is_err());
        assert!(require_reported_identity_is_own_v1(path, b"not json").is_err());
    }

    /// Predecessor program sections never survive into an arm record, even
    /// when the parent carries one: the cycle-4 validator's mutual exclusion
    /// would reject it, and dropping them is what makes a cycle-3 parent
    /// usable at all.
    #[test]
    fn predecessor_program_sections_are_dropped_v1() {
        let built = build_v1(Cycle4ArmKindV1::StaticRb);
        let contracts = built.record().contracts();
        assert!(contracts.population_program_v1.is_none());
        assert!(contracts.response_exploiter_v1.is_none());
        assert!(contracts.population_program_v2_cycle2.is_none());
        assert!(contracts.population_program_v2_cycle3.is_none());
        assert!(contracts.wide_model_experiment_v1.is_none());
    }
}
