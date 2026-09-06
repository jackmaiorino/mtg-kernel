//! Launcher-level builder for one cycle-5 arm's `run.json`.
//!
//! Mirrors `native_cycle4_run_record_v1` (documented in
//! `docs/native_cycle4_arm_launcher_v1.md` Section 1) for the cycle-5
//! program whose parent the immutable cycle-4 routing record names
//! (`OX_CYCLE5_PREREG_SKETCH_V1.md`, an UNRATIFIED draft; see the sentinel
//! literals in `native_training_store_run_v2`). It builds the record,
//! deterministically, from exactly four sources and nothing else:
//!
//! 1. the ARM KIND, which decides `population_program_v2_cycle5.arm_kind`
//!    and the arm's own formal base seed; `control-v3` is the frozen v3
//!    recipe continued from the parent, `centered-v5` is declared but
//!    refused until the v5 trainer contract is ratified;
//! 2. the ROUTING RECORD (`routing-record.json`, whose bytes must hash to
//!    the pinned `CYCLE5_ROUTING_RECORD_SHA256_V1`), which names the parent
//!    run, the parent checkpoint manifest and the parent generation; the
//!    builder cross-checks all three against what it stages from the parent
//!    Store, so an operator-typed generation can never seed cycle 5 from
//!    anything but the routed parent;
//! 3. the PARENT Store (the cycle-3 lineage run at its own generation 2048),
//!    which supplies the whole device contract, train step, model
//!    architecture, schedule shape, environment, toolchain and runtime
//!    tuple, plus the six pinned digests of
//!    `contracts.opponent_ladder_initialization`, resolved through the same
//!    `stage_ladder_checkpoint_initialization_v1` the arm bin's genesis
//!    bootstrap re-derives;
//! 4. the PINNED cycle-5 literals compiled into
//!    `native_training_store_run_v2` (pre-registration digest, refresh
//!    manifest schema, 2048/4096/128, requested successful updates),
//!    imported rather than restated.
//!
//! Nothing else enters: no clock, environment variable, random source or
//! operator-chosen field, so two invocations against the same parent Store
//! and routing record produce byte-identical output.
//!
//! The formal base seed literals are NOT yet ratified. They live on
//! [`Cycle5ArmKindV1::formal_base_seed_v1`], which returns an unratified
//! placeholder in production builds; the arm launcher's own record-level
//! validator refuses that placeholder, so no cycle-5 record can launch until
//! the owner ratifies the seed bands and the literal is replaced.
//!
//! Everything the builder produces is passed through
//! `validate_train_run_record_v2` and then through
//! `native_cycle5_arm_v1::validate_cycle5_arm_run_record_v1` BEFORE any
//! bytes are returned.

use crate::native_cycle5_arm_v1::{validate_cycle5_arm_run_record_v1, Cycle5ArmKindV1};
use crate::native_ladder_pool_resolution_v1::stage_ladder_checkpoint_initialization_v1;
use crate::native_policy_train_step_v1::CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1;
use crate::native_store_production_capture_v2::{
    capture_launcher_build_provenance_v2, current_launcher_build_identity_v2,
    decode_launcher_build_identity_v2, LauncherBuildProvenanceV2,
};
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
use crate::native_training_store_run_v2::{
    decode_train_run_v2, refresh_derived_fields_v2, validate_train_run_record_v2,
    OpponentLadderInitializationContractV1, PopulationProgramContractV2Cycle5, TrainRunContractsV2,
    TrainRunScheduleV2, TrainRunV2, ValidatedTrainRunV2, CUDA_RUNTIME_TUPLE_IDENTITY_V2,
    CYCLE5_ARM_LAUNCHER_BINARY_NAME_V1, CYCLE5_PREREG_SHA256_V1, CYCLE5_REFRESH_INTERVAL_V1,
    CYCLE5_REFRESH_MANIFEST_SCHEMA_V1, CYCLE5_ROUTING_RECORD_SCHEMA_V1,
    CYCLE5_ROUTING_RECORD_SHA256_V1, CYCLE5_TOTAL_SUCCESSFUL_UPDATES_V1,
    CYCLE5_TRAINEE_START_GENERATION_V1, CYCLE5_TRAINEE_STOP_GENERATION_V1,
    NATIVE_TRAINING_STORE_IDENTITY_V2, TRAIN_RUN_SCHEMA_V2,
};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The routing record's expected outcome: cycle 5 exists because nothing
/// carried out of cycle 4.
const CYCLE5_ROUTING_RECORD_OUTCOME_V1: &str = "NO_CARRY";
/// The recipe the routing record names for the parent (the frozen v3 loss
/// under the cycle-4 CONTROL-R arm); the cycle-5 `control-v3` arm continues
/// exactly this recipe.
const CYCLE5_ROUTING_RECIPE_ARM_KIND_V1: &str = "control-r";
const CYCLE5_ROUTING_RECIPE_LOSS_IDENTITY_V1: &str = "terminal_reinforce_value/v3";

/// The arm's formal training base seed (see
/// [`Cycle5ArmKindV1::formal_base_seed_v1`] for the ratification state).
#[must_use]
pub const fn cycle5_arm_base_seed_v1(arm: Cycle5ArmKindV1) -> u64 {
    arm.formal_base_seed_v1()
}

/// One run-record build request. Every field is required; there are no
/// defaults and no environment lookups.
#[derive(Clone, Debug)]
pub struct Cycle5RunRecordRequestV1 {
    pub arm: Cycle5ArmKindV1,
    /// The immutable cycle-4 routing record that names the parent.
    pub routing_record_path: PathBuf,
    /// The cycle-3 lineage Store the arm's genesis weights come from (the
    /// routing record's `parent_run_sha256`).
    pub parent_store_root: PathBuf,
    /// The parent's generation in ITS OWN Store numbering: 2048, the routing
    /// record's `parent_store_generation`. Any other value is rejected before
    /// the parent is even staged.
    pub parent_generation: u64,
    /// The cycle-5 arm launcher that will publish the Store
    /// (`cycle5_arm_v1.exe`).
    pub arm_executable: PathBuf,
}

/// What one build produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cycle5RunRecordOutcomeV1 {
    pub arm_kind: String,
    pub base_seed: u64,
    /// SHA-256 of `canonical_bytes`, which is the arm's `run_sha256`.
    pub run_sha256: String,
    pub canonical_bytes: Vec<u8>,
    /// The parent record's own `run_sha256`, as resolved from the parent
    /// Store and as the routing record names it.
    pub parent_run_sha256: String,
    /// The parent checkpoint manifest digest, as staged and as the routing
    /// record names it.
    pub parent_checkpoint_manifest_sha256: String,
    pub parent_generation: u64,
    /// SHA-256 of the routing record bytes the build read, restated so the
    /// launch manifest can bind it.
    pub routing_record_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cycle5RunRecordErrorV1 {
    /// The arm kind is declared but cannot be built yet.
    ArmUnratified { arm: String, detail: String },
    /// The routing record could not be read.
    RoutingRecordIo { path: PathBuf, detail: String },
    /// The routing record is not the pinned, immutable record, or names a
    /// parent that does not match the request or the staged parent.
    RoutingRecordRejected { detail: String },
    /// The parent Store's `run.json` could not be read.
    ParentRunFileIo { path: PathBuf, detail: String },
    /// The parent Store's `run.json` failed V2 record validation.
    ParentRunRejected { detail: String },
    /// `--parent-generation` named a generation cycle 5 does not start from.
    ParentGenerationNotPinned { requested: u64, required: u64 },
    /// The pinned parent checkpoint at `--parent-generation` could not be
    /// resolved from the parent Store.
    ParentCheckpointRejected { detail: String },
    /// The arm launcher executable could not be captured.
    ArmExecutableRejected { path: PathBuf, detail: String },
    /// The arm launcher did not report this build's own identity.
    ArmBuildIdentityMismatch { path: PathBuf, detail: String },
    /// The parent record does not carry the ladder tuple a cycle-5 arm needs.
    ParentMissingLadderTuple { section: &'static str },
    /// The assembled record failed `validate_train_run_record_v2`.
    RecordRejected { detail: String },
    /// The assembled record failed the launcher's own record-level check.
    ArmContractRejected { code: String, detail: String },
}

impl Display for Cycle5RunRecordErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ArmUnratified { arm, detail } => {
                write!(formatter, "arm {arm} cannot be built yet: {detail}")
            }
            Self::RoutingRecordIo { path, detail } => {
                write!(formatter, "routing record unreadable ({}): {detail}", path.display())
            }
            Self::RoutingRecordRejected { detail } => {
                write!(formatter, "routing record rejected: {detail}")
            }
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
                "parent generation {requested} is not the routed cycle-5 start {required}; a cycle-5 arm is seeded from the routing record's parent at g{required}"
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
                "the parent run record carries no contracts.{section}; a cycle-5 arm record cannot be built from it"
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

impl Error for Cycle5RunRecordErrorV1 {}

type Result<T> = std::result::Result<T, Cycle5RunRecordErrorV1>;

/// The parent as the routing record names it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cycle5RoutedParentV1 {
    pub parent_run_sha256: String,
    pub parent_checkpoint_manifest_sha256: String,
    pub parent_store_generation: u64,
}

/// Requires the routing record bytes to be the pinned, immutable record.
///
/// # Errors
///
/// Returns [`Cycle5RunRecordErrorV1::RoutingRecordRejected`] when the bytes
/// hash to anything but `CYCLE5_ROUTING_RECORD_SHA256_V1`.
pub fn require_routing_record_digest_v1(bytes: &[u8]) -> Result<String> {
    let digest = lower_hex_raw32_v1(sha256_v1(bytes));
    if digest != CYCLE5_ROUTING_RECORD_SHA256_V1 {
        return Err(Cycle5RunRecordErrorV1::RoutingRecordRejected {
            detail: format!(
                "bytes hash to {digest}, not the pinned routing record {CYCLE5_ROUTING_RECORD_SHA256_V1}"
            ),
        });
    }
    Ok(digest)
}

/// Decodes the parent the routing record names and checks the record's own
/// claims: the schema, the NO_CARRY outcome, the v3 recipe, and that the
/// parent generation equals both the pinned cycle-5 start and the requested
/// one. Pure over the bytes, so it is testable without the real file; the
/// digest gate is [`require_routing_record_digest_v1`].
///
/// # Errors
///
/// Returns [`Cycle5RunRecordErrorV1::RoutingRecordRejected`] on any
/// malformed or mismatching field.
pub fn decode_routed_parent_v1(bytes: &[u8], requested_generation: u64) -> Result<Cycle5RoutedParentV1> {
    let reject = |detail: String| Cycle5RunRecordErrorV1::RoutingRecordRejected { detail };
    let document: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| reject(format!("not JSON: {error}")))?;
    let field = |name: &str| {
        document
            .get(name)
            .ok_or_else(|| reject(format!("missing field {name}")))
    };
    let string_field = |name: &str| -> Result<String> {
        field(name)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| reject(format!("field {name} is not a string")))
    };
    let schema = string_field("schema")?;
    if schema != CYCLE5_ROUTING_RECORD_SCHEMA_V1 {
        return Err(reject(format!("schema {schema} is not {CYCLE5_ROUTING_RECORD_SCHEMA_V1}")));
    }
    let outcome = string_field("outcome")?;
    if outcome != CYCLE5_ROUTING_RECORD_OUTCOME_V1 {
        return Err(reject(format!(
            "outcome {outcome} is not {CYCLE5_ROUTING_RECORD_OUTCOME_V1}; cycle 5 is defined by the no-carry route"
        )));
    }
    let recipe = field("recipe")?;
    let recipe_string = |name: &str| -> Result<String> {
        recipe
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| reject(format!("recipe.{name} missing or not a string")))
    };
    if recipe_string("arm_kind")? != CYCLE5_ROUTING_RECIPE_ARM_KIND_V1
        || recipe_string("trainer_loss_identity")? != CYCLE5_ROUTING_RECIPE_LOSS_IDENTITY_V1
        || recipe.get("centered_baseline").and_then(serde_json::Value::as_bool) != Some(false)
    {
        return Err(reject(
            "recipe is not the frozen v3 control recipe the cycle-5 control arm continues".to_owned(),
        ));
    }
    let parent_run_sha256 = string_field("parent_run_sha256")?;
    let parent_checkpoint_manifest_sha256 = string_field("parent_checkpoint_manifest_sha256")?;
    let is_sha256 = |value: &str| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
    if !is_sha256(&parent_run_sha256) || !is_sha256(&parent_checkpoint_manifest_sha256) {
        return Err(reject("parent digests are not lowercase SHA-256".to_owned()));
    }
    let parent_store_generation = field("parent_store_generation")?
        .as_u64()
        .ok_or_else(|| reject("parent_store_generation is not an unsigned integer".to_owned()))?;
    if parent_store_generation != CYCLE5_TRAINEE_START_GENERATION_V1 {
        return Err(reject(format!(
            "parent_store_generation {parent_store_generation} is not the pinned cycle-5 start {CYCLE5_TRAINEE_START_GENERATION_V1}"
        )));
    }
    if parent_store_generation != requested_generation {
        return Err(reject(format!(
            "the request names parent generation {requested_generation} but the routing record names {parent_store_generation}"
        )));
    }
    Ok(Cycle5RoutedParentV1 {
        parent_run_sha256,
        parent_checkpoint_manifest_sha256,
        parent_store_generation,
    })
}

/// The `population_program_v2_cycle5` section, wholly from the pinned
/// literals plus the arm kind.
fn population_program_section_v1(arm: Cycle5ArmKindV1) -> PopulationProgramContractV2Cycle5 {
    PopulationProgramContractV2Cycle5 {
        prereg_sha256: CYCLE5_PREREG_SHA256_V1.to_owned(),
        refresh_manifest_schema: CYCLE5_REFRESH_MANIFEST_SCHEMA_V1.to_owned(),
        arm_kind: arm.wire_v1().to_owned(),
        trainee_start_generation: CYCLE5_TRAINEE_START_GENERATION_V1,
        trainee_stop_generation: CYCLE5_TRAINEE_STOP_GENERATION_V1,
        refresh_interval: CYCLE5_REFRESH_INTERVAL_V1,
        static_pool: arm.static_pool_v1(),
    }
}

/// The centered-v5 arm is declared so records and rosters can name it, but
/// no record can be built for it until the v5 trainer contract is ratified
/// and its section builder exists. Fail closed before any I/O.
fn require_arm_is_buildable_v1(arm: Cycle5ArmKindV1) -> Result<()> {
    if arm.uses_centered_baseline_v1() {
        return Err(Cycle5RunRecordErrorV1::ArmUnratified {
            arm: arm.wire_v1().to_owned(),
            detail: "the v5 trainer contract is not ratified; only control-v3 can be built"
                .to_owned(),
        });
    }
    Ok(())
}

/// Requires the arm launcher to report exactly THIS builder's own embedded
/// build identity (see the cycle-4 module for the rationale).
///
/// # Errors
///
/// Returns [`Cycle5RunRecordErrorV1::ArmBuildIdentityMismatch`] if the child
/// cannot be run, exits nonzero, prints something that is not a canonical
/// identity, or reports an identity that is not this build's.
fn require_arm_executable_build_identity_v1(arm_executable: &Path) -> Result<()> {
    let reject = |detail: String| Cycle5RunRecordErrorV1::ArmBuildIdentityMismatch {
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

/// The decision half of [`require_arm_executable_build_identity_v1`].
fn require_reported_identity_is_own_v1(arm_executable: &Path, stdout: &[u8]) -> Result<()> {
    let reject = |detail: String| Cycle5RunRecordErrorV1::ArmBuildIdentityMismatch {
        path: arm_executable.to_path_buf(),
        detail,
    };
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
/// Returns a [`Cycle5RunRecordErrorV1`] if the arm cannot be built yet, if
/// the routing record is not the pinned one or disagrees with the request or
/// the staged parent, if the parent Store cannot be read or resolved, if the
/// parent record lacks the ladder tuple, or if the assembled record is
/// rejected by either validator. Nothing is written by this function.
pub fn build_cycle5_arm_run_record_v1(
    request: &Cycle5RunRecordRequestV1,
) -> Result<Cycle5RunRecordOutcomeV1> {
    require_arm_is_buildable_v1(request.arm)?;
    if request.parent_generation != CYCLE5_TRAINEE_START_GENERATION_V1 {
        return Err(Cycle5RunRecordErrorV1::ParentGenerationNotPinned {
            requested: request.parent_generation,
            required: CYCLE5_TRAINEE_START_GENERATION_V1,
        });
    }

    // The routing record first: its bytes must be the pinned record, and its
    // parent fields are what the staged parent is checked against below.
    let routing_bytes = std::fs::read(&request.routing_record_path).map_err(|error| {
        Cycle5RunRecordErrorV1::RoutingRecordIo {
            path: request.routing_record_path.clone(),
            detail: error.to_string(),
        }
    })?;
    let routing_record_sha256 = require_routing_record_digest_v1(&routing_bytes)?;
    let routed = decode_routed_parent_v1(&routing_bytes, request.parent_generation)?;

    let parent_root: &Path = request.parent_store_root.as_path();
    let parent_run_path = parent_root.join("run.json");
    let parent_bytes = std::fs::read(&parent_run_path).map_err(|error| {
        Cycle5RunRecordErrorV1::ParentRunFileIo {
            path: parent_run_path.clone(),
            detail: error.to_string(),
        }
    })?;
    let parent = decode_train_run_v2(&parent_bytes).map_err(|error| {
        Cycle5RunRecordErrorV1::ParentRunRejected {
            detail: error.to_string(),
        }
    })?;
    if parent.run_sha256() != routed.parent_run_sha256 {
        return Err(Cycle5RunRecordErrorV1::RoutingRecordRejected {
            detail: format!(
                "the parent Store's run is {} but the routing record names {}",
                parent.run_sha256(),
                routed.parent_run_sha256
            ),
        });
    }

    let initialization =
        stage_ladder_checkpoint_initialization_v1(parent_root, request.parent_generation).map_err(
            |error| Cycle5RunRecordErrorV1::ParentCheckpointRejected {
                detail: format!(
                    "{} generation {}: {error}",
                    parent_root.display(),
                    request.parent_generation
                ),
            },
        )?;
    require_initialization_matches_route_v1(&initialization, &routed)?;
    let parent_run_sha256 = initialization.source_run_sha256.clone();
    let parent_checkpoint_manifest_sha256 = initialization.checkpoint_sha256.clone();

    require_arm_executable_build_identity_v1(request.arm_executable.as_path())?;
    let provenance = capture_launcher_build_provenance_v2(
        request.arm_executable.as_path(),
        CYCLE5_ARM_LAUNCHER_BINARY_NAME_V1,
        CUDA_RUNTIME_TUPLE_IDENTITY_V2,
        CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1,
    )
    .map_err(|error| Cycle5RunRecordErrorV1::ArmExecutableRejected {
        path: request.arm_executable.clone(),
        detail: error.to_string(),
    })?;

    let validated =
        assemble_cycle5_arm_run_record_v1(request.arm, &parent, initialization, provenance)?;
    Ok(Cycle5RunRecordOutcomeV1 {
        arm_kind: request.arm.wire_v1().to_owned(),
        base_seed: cycle5_arm_base_seed_v1(request.arm),
        run_sha256: validated.run_sha256().to_owned(),
        canonical_bytes: validated.canonical_bytes().to_vec(),
        parent_run_sha256,
        parent_checkpoint_manifest_sha256,
        parent_generation: request.parent_generation,
        routing_record_sha256,
    })
}

/// The staged genesis origin must be the parent the routing record names,
/// field by field: run, generation and checkpoint manifest.
fn require_initialization_matches_route_v1(
    initialization: &OpponentLadderInitializationContractV1,
    routed: &Cycle5RoutedParentV1,
) -> Result<()> {
    if initialization.source_run_sha256 != routed.parent_run_sha256
        || initialization.generation != routed.parent_store_generation
        || initialization.checkpoint_sha256 != routed.parent_checkpoint_manifest_sha256
    {
        return Err(Cycle5RunRecordErrorV1::RoutingRecordRejected {
            detail: format!(
                "the staged parent (run {}, generation {}, checkpoint {}) is not the routed parent (run {}, generation {}, checkpoint {})",
                initialization.source_run_sha256,
                initialization.generation,
                initialization.checkpoint_sha256,
                routed.parent_run_sha256,
                routed.parent_store_generation,
                routed.parent_checkpoint_manifest_sha256
            ),
        });
    }
    Ok(())
}

/// The pure half of the build: everything from an already-decoded parent
/// record and an already-resolved genesis origin to a validated cycle-5 arm
/// record.
fn assemble_cycle5_arm_run_record_v1(
    arm: Cycle5ArmKindV1,
    parent: &ValidatedTrainRunV2,
    initialization: OpponentLadderInitializationContractV1,
    provenance: LauncherBuildProvenanceV2,
) -> Result<ValidatedTrainRunV2> {
    assemble_with_base_seed_v1(
        arm,
        parent,
        initialization,
        provenance,
        cycle5_arm_base_seed_v1(arm),
    )
}

/// The assembly body, with the base seed as an explicit parameter so a test
/// can present a record carrying the wrong seed and prove the launcher's own
/// validator refuses it.
fn assemble_with_base_seed_v1(
    arm: Cycle5ArmKindV1,
    parent: &ValidatedTrainRunV2,
    initialization: OpponentLadderInitializationContractV1,
    provenance: LauncherBuildProvenanceV2,
    base_seed: u64,
) -> Result<ValidatedTrainRunV2> {
    require_arm_is_buildable_v1(arm)?;
    if initialization.generation != CYCLE5_TRAINEE_START_GENERATION_V1 {
        return Err(Cycle5RunRecordErrorV1::ParentGenerationNotPinned {
            requested: initialization.generation,
            required: CYCLE5_TRAINEE_START_GENERATION_V1,
        });
    }
    let parent_record = parent.record();
    let parent_contracts = &parent_record.contracts;
    let ladder_pool = parent_contracts.opponent_ladder_pool.clone().ok_or(
        Cycle5RunRecordErrorV1::ParentMissingLadderTuple {
            section: "opponent_ladder_pool",
        },
    )?;
    let opponent_schedule = parent_contracts.opponent_schedule_v2.clone().ok_or(
        Cycle5RunRecordErrorV1::ParentMissingLadderTuple {
            section: "opponent_schedule_v2",
        },
    )?;

    // The control arm keeps the parent's frozen v3 loss; the centered arm
    // was refused above.
    let loss = parent_contracts.loss.clone();
    let mut train_step = parent_contracts.train_step.clone();
    train_step.numerical_backend_identity =
        CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1.to_owned();

    let contracts = TrainRunContractsV2 {
        trainer_identity: parent_contracts.trainer_identity.clone(),
        identity_bundle_identity: parent_contracts.identity_bundle_identity.clone(),
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
        standalone_semantics: parent_contracts.standalone_semantics.clone(),
        // Every predecessor program section is DROPPED, not carried; the
        // cycle-5 validator refuses a record carrying any of them.
        wide_model_experiment_v1: None,
        population_program_v1: None,
        response_exploiter_v1: None,
        population_program_v2_cycle2: None,
        population_program_v2_cycle3: None,
        trainer_v4_candidate: None,
        population_program_v2_cycle4: None,
        trainer_v5_candidate: None,
        population_program_v2_cycle5: Some(population_program_section_v1(arm)),
    };

    let schedule = TrainRunScheduleV2 {
        base_seed,
        requested_successful_updates: CYCLE5_TOTAL_SUCCESSFUL_UPDATES_V1,
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
        Cycle5RunRecordErrorV1::RecordRejected {
            detail: error.to_string(),
        }
    })?;

    let validated = validate_train_run_record_v2(record).map_err(|error| {
        Cycle5RunRecordErrorV1::RecordRejected {
            detail: error.to_string(),
        }
    })?;
    validate_cycle5_arm_run_record_v1(&validated, arm).map_err(|error| {
        Cycle5RunRecordErrorV1::ArmContractRejected {
            code: error.code_v1().to_owned(),
            detail: error.to_string(),
        }
    })?;

    Ok(validated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_json_v1::{to_canonical_json_bytes_v1, CanonicalJsonNullPolicyV1};
    use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
    use crate::native_store_production_capture_v2::{
        current_launcher_build_identity_json_v2, require_run_record_matches_provenance_v2,
        test_launcher_build_provenance_v2, LauncherBuildIdentityV2,
    };
    use crate::native_training_store_run_v2::{
        test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2,
        test_fixture_ladder_initialization_v1, test_fixture_ladder_pool_v2,
    };

    fn identity_bytes_v1(identity: &LauncherBuildIdentityV2) -> Vec<u8> {
        to_canonical_json_bytes_v1(identity, CanonicalJsonNullPolicyV1::Forbid)
            .expect("a build identity encodes canonically")
    }

    /// The pinned genesis origin at the routed start generation.
    fn pinned_origin_v1() -> OpponentLadderInitializationContractV1 {
        let mut initialization = test_fixture_ladder_initialization_v1();
        initialization.generation = CYCLE5_TRAINEE_START_GENERATION_V1;
        initialization
    }

    fn provenance_v1() -> LauncherBuildProvenanceV2 {
        test_launcher_build_provenance_v2(
            CYCLE5_ARM_LAUNCHER_BINARY_NAME_V1,
            CUDA_RUNTIME_TUPLE_IDENTITY_V2,
            CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1,
        )
    }

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

    fn build_v1(arm: Cycle5ArmKindV1) -> ValidatedTrainRunV2 {
        assemble_cycle5_arm_run_record_v1(arm, &parent_v1(), pinned_origin_v1(), provenance_v1())
            .expect("the assembled cycle-5 arm record must validate")
    }

    /// A routing record shaped like the real one, with the requested parent.
    fn routing_record_json_v1(run: &str, checkpoint: &str, generation: u64) -> Vec<u8> {
        let document = serde_json::json!({
            "schema": CYCLE5_ROUTING_RECORD_SCHEMA_V1,
            "outcome": CYCLE5_ROUTING_RECORD_OUTCOME_V1,
            "parent_run_sha256": run,
            "parent_checkpoint_manifest_sha256": checkpoint,
            "parent_store_generation": generation,
            "recipe": {
                "arm_kind": CYCLE5_ROUTING_RECIPE_ARM_KIND_V1,
                "trainer_loss_identity": CYCLE5_ROUTING_RECIPE_LOSS_IDENTITY_V1,
                "centered_baseline": false,
                "refresh_machinery": true
            }
        });
        serde_json::to_vec(&document).expect("json")
    }

    #[test]
    fn only_the_routed_parent_generation_is_admissible_v1() {
        for generation in [0_u64, 384, 896, 2047, 2049] {
            let mut initialization = pinned_origin_v1();
            initialization.generation = generation;
            let error = assemble_cycle5_arm_run_record_v1(
                Cycle5ArmKindV1::ControlV3,
                &parent_v1(),
                initialization,
                provenance_v1(),
            )
            .expect_err("a parent generation other than 2048 must be refused");
            assert_eq!(
                error,
                Cycle5RunRecordErrorV1::ParentGenerationNotPinned {
                    requested: generation,
                    required: CYCLE5_TRAINEE_START_GENERATION_V1,
                }
            );
        }
        assert_eq!(
            build_v1(Cycle5ArmKindV1::ControlV3)
                .record()
                .contracts()
                .opponent_ladder_initialization
                .as_ref()
                .expect("pinned origin")
                .generation,
            CYCLE5_TRAINEE_START_GENERATION_V1
        );
    }

    #[test]
    fn the_centered_arm_is_refused_before_any_io_v1() {
        let error = assemble_cycle5_arm_run_record_v1(
            Cycle5ArmKindV1::CenteredV5,
            &parent_v1(),
            pinned_origin_v1(),
            provenance_v1(),
        )
        .expect_err("centered-v5 must fail closed until ratified");
        assert!(matches!(error, Cycle5RunRecordErrorV1::ArmUnratified { .. }));
        let request = Cycle5RunRecordRequestV1 {
            arm: Cycle5ArmKindV1::CenteredV5,
            routing_record_path: PathBuf::from("Z:\\does-not-exist\\routing-record.json"),
            parent_store_root: PathBuf::from("Z:\\does-not-exist\\store"),
            parent_generation: CYCLE5_TRAINEE_START_GENERATION_V1,
            arm_executable: PathBuf::from("Z:\\does-not-exist\\cycle5_arm_v1.exe"),
        };
        // Refused by the arm gate, not by the missing files: no I/O happened.
        assert!(matches!(
            build_cycle5_arm_run_record_v1(&request).expect_err("refused"),
            Cycle5RunRecordErrorV1::ArmUnratified { .. }
        ));
    }

    #[test]
    fn the_routing_record_digest_gate_refuses_other_bytes_v1() {
        let bytes = routing_record_json_v1(&"a".repeat(64), &"b".repeat(64), 2048);
        let error = require_routing_record_digest_v1(&bytes).expect_err("not the pinned record");
        assert!(matches!(error, Cycle5RunRecordErrorV1::RoutingRecordRejected { .. }));
    }

    #[test]
    fn the_routed_parent_is_decoded_and_cross_checked_v1() {
        let run = "a".repeat(64);
        let checkpoint = "b".repeat(64);
        let routed = decode_routed_parent_v1(&routing_record_json_v1(&run, &checkpoint, 2048), 2048)
            .expect("a well-formed record decodes");
        assert_eq!(routed.parent_run_sha256, run);
        assert_eq!(routed.parent_checkpoint_manifest_sha256, checkpoint);
        assert_eq!(routed.parent_store_generation, 2048);

        // A request naming another generation is refused against the record.
        assert!(decode_routed_parent_v1(&routing_record_json_v1(&run, &checkpoint, 2048), 896).is_err());
        // A record naming another generation is refused outright.
        assert!(decode_routed_parent_v1(&routing_record_json_v1(&run, &checkpoint, 896), 896).is_err());
        // Wrong schema, outcome or recipe.
        let mut wrong_outcome: serde_json::Value =
            serde_json::from_slice(&routing_record_json_v1(&run, &checkpoint, 2048)).unwrap();
        wrong_outcome["outcome"] = serde_json::json!("CARRY");
        assert!(decode_routed_parent_v1(&serde_json::to_vec(&wrong_outcome).unwrap(), 2048).is_err());
        let mut wrong_recipe: serde_json::Value =
            serde_json::from_slice(&routing_record_json_v1(&run, &checkpoint, 2048)).unwrap();
        wrong_recipe["recipe"]["centered_baseline"] = serde_json::json!(true);
        assert!(decode_routed_parent_v1(&serde_json::to_vec(&wrong_recipe).unwrap(), 2048).is_err());
        assert!(decode_routed_parent_v1(b"not json", 2048).is_err());

        // The staged origin must agree with the route field by field.
        let mut initialization = pinned_origin_v1();
        initialization.source_run_sha256 = run.clone();
        initialization.checkpoint_sha256 = checkpoint.clone();
        require_initialization_matches_route_v1(&initialization, &routed).expect("matches");
        let mut other_checkpoint = initialization.clone();
        other_checkpoint.checkpoint_sha256 = "c".repeat(64);
        assert!(require_initialization_matches_route_v1(&other_checkpoint, &routed).is_err());
        let mut other_run = initialization;
        other_run.source_run_sha256 = "c".repeat(64);
        assert!(require_initialization_matches_route_v1(&other_run, &routed).is_err());
    }

    /// The real routing record, when this machine has it: the digest gate and
    /// the decoder agree with the pinned constants. Ignored elsewhere.
    #[test]
    #[ignore]
    fn the_real_routing_record_is_the_pinned_one_v1() {
        let path = Path::new("E:\\mtg-kernel-cycle4-arms-lead\\routing\\routing-record.json");
        let bytes = std::fs::read(path).expect("the routing record exists on the campaign host");
        require_routing_record_digest_v1(&bytes).expect("pinned digest");
        let routed = decode_routed_parent_v1(&bytes, CYCLE5_TRAINEE_START_GENERATION_V1)
            .expect("the real record decodes");
        assert_eq!(routed.parent_store_generation, CYCLE5_TRAINEE_START_GENERATION_V1);
    }

    #[test]
    fn provenance_is_the_current_build_not_the_parents_v1() {
        let parent = parent_v1();
        let built = assemble_cycle5_arm_run_record_v1(
            Cycle5ArmKindV1::ControlV3,
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
        assert_eq!(record.source.binary_name, CYCLE5_ARM_LAUNCHER_BINARY_NAME_V1);
        assert_eq!(record.runtime.tuple_identity, CUDA_RUNTIME_TUPLE_IDENTITY_V2);
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
    /// record-level validator, not only by the builder.
    #[test]
    fn the_launcher_rejects_a_record_carrying_another_seed_v1() {
        let arm = Cycle5ArmKindV1::ControlV3;
        let wrong_seed = arm.formal_base_seed_v1() + 1;
        let error = assemble_with_base_seed_v1(
            arm,
            &parent_v1(),
            pinned_origin_v1(),
            provenance_v1(),
            wrong_seed,
        )
        .expect_err("another base seed must be refused");
        assert_eq!(
            error,
            Cycle5RunRecordErrorV1::ArmContractRejected {
                code: "cycle5_arm_v1_base_seed_mismatch".to_owned(),
                detail: format!(
                    "cycle5_arm_v1_base_seed_mismatch: {} trains under base seed {}, but the run record declares {wrong_seed}",
                    arm.wire_v1(),
                    arm.formal_base_seed_v1()
                ),
            }
        );
    }

    #[test]
    fn the_control_arm_assembles_a_record_that_validates_v1() {
        let arm = Cycle5ArmKindV1::ControlV3;
        let built = build_v1(arm);
        let program = built
            .record()
            .contracts()
            .population_program_v2_cycle5
            .as_ref()
            .expect("the cycle-5 section is required");
        assert_eq!(program.arm_kind, arm.wire_v1());
        assert!(!program.static_pool);
        assert_eq!(program.prereg_sha256, CYCLE5_PREREG_SHA256_V1);
        assert_eq!(program.trainee_start_generation, CYCLE5_TRAINEE_START_GENERATION_V1);
        assert_eq!(program.trainee_stop_generation, CYCLE5_TRAINEE_STOP_GENERATION_V1);
        assert_eq!(program.refresh_interval, CYCLE5_REFRESH_INTERVAL_V1);
        assert_eq!(built.requested_successful_updates(), CYCLE5_TOTAL_SUCCESSFUL_UPDATES_V1);
        assert_eq!(built.record().schedule().base_seed, cycle5_arm_base_seed_v1(arm));
        let contracts = built.record().contracts();
        assert!(contracts.trainer_v4_candidate.is_none());
        assert!(contracts.trainer_v5_candidate.is_none());
        assert!(contracts.population_program_v2_cycle4.is_none());
        assert!(contracts.population_program_v1.is_none());
        assert!(contracts.response_exploiter_v1.is_none());
        assert!(contracts.population_program_v2_cycle2.is_none());
        assert!(contracts.population_program_v2_cycle3.is_none());
        assert!(contracts.wide_model_experiment_v1.is_none());
        assert!(contracts.opponent_ladder_initialization.is_some());
        let decoded = decode_train_run_v2(built.canonical_bytes()).expect("the published bytes must decode");
        assert_eq!(decoded.run_sha256(), built.run_sha256());
    }

    #[test]
    fn a_tampered_field_is_rejected_v1() {
        let built = build_v1(Cycle5ArmKindV1::ControlV3);
        let text = String::from_utf8(built.canonical_bytes().to_vec()).expect("utf-8 record");

        let arm_swapped = text.replace(r#""arm_kind":"control-v3""#, r#""arm_kind":"centered-v5""#);
        assert_ne!(arm_swapped, text);
        assert!(decode_train_run_v2(arm_swapped.as_bytes()).is_err());

        let prereg_swapped = text.replace(CYCLE5_PREREG_SHA256_V1, &"0".repeat(64));
        assert_ne!(prereg_swapped, text);
        assert!(decode_train_run_v2(prereg_swapped.as_bytes()).is_err());

        let span_swapped = text.replace(
            &format!(r#""trainee_stop_generation":{CYCLE5_TRAINEE_STOP_GENERATION_V1}"#),
            &format!(r#""trainee_stop_generation":{CYCLE5_TRAINEE_START_GENERATION_V1}"#),
        );
        assert_ne!(span_swapped, text);
        assert!(decode_train_run_v2(span_swapped.as_bytes()).is_err());

        let seed_swapped = text.replace(
            &format!(r#""base_seed":{}"#, cycle5_arm_base_seed_v1(Cycle5ArmKindV1::ControlV3)),
            r#""base_seed":123456"#,
        );
        assert_ne!(seed_swapped, text);
        assert!(decode_train_run_v2(seed_swapped.as_bytes()).is_err());
    }

    #[test]
    fn two_invocations_are_byte_identical_v1() {
        let first = build_v1(Cycle5ArmKindV1::ControlV3);
        let second = build_v1(Cycle5ArmKindV1::ControlV3);
        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
        assert_eq!(first.run_sha256(), second.run_sha256());
    }

    #[test]
    fn an_arm_from_a_different_build_refuses_the_record_v1() {
        let built = build_v1(Cycle5ArmKindV1::ControlV3);
        require_run_record_matches_provenance_v2(&built, &provenance_v1())
            .expect("the record's own build must be accepted");
        let mut other_commit = provenance_v1();
        other_commit.source.git_commit = "1111111111111111111111111111111111111111".to_owned();
        let error = require_run_record_matches_provenance_v2(&built, &other_commit)
            .expect_err("a record from another commit must be refused");
        assert_eq!(error.code(), "run_record_source_is_not_this_build");
        let mut other_binary = provenance_v1();
        other_binary.source.binary_sha256 = "0".repeat(64);
        let error = require_run_record_matches_provenance_v2(&built, &other_binary)
            .expect_err("a record naming another binary must be refused");
        assert_eq!(error.code(), "run_record_source_is_not_this_build");
        let mut other_package = provenance_v1();
        other_package.package.cargo_lock_sha256 = "0".repeat(64);
        let error = require_run_record_matches_provenance_v2(&built, &other_package)
            .expect_err("a record from another build's inputs must be refused");
        assert_eq!(error.code(), "run_record_package_is_not_this_build");
    }

    #[test]
    fn the_builder_refuses_an_arm_from_a_different_build_v1() {
        let path = Path::new("D:\\release\\cycle5_arm_v1.exe");
        let own = current_launcher_build_identity_v2();
        let own_json =
            current_launcher_build_identity_json_v2().expect("this build reports an identity");
        assert_eq!(identity_bytes_v1(&own), own_json.as_bytes());
        require_reported_identity_is_own_v1(path, own_json.as_bytes())
            .expect("the builder's own identity must be accepted");
        require_reported_identity_is_own_v1(path, own_json.replace('\n', "\r\n").as_bytes())
            .expect("a CRLF-reframed identity is still this build's");
        let mut other_commit = own.clone();
        other_commit.source_git_commit = "1".repeat(40);
        let error = require_reported_identity_is_own_v1(path, &identity_bytes_v1(&other_commit))
            .expect_err("another commit must be refused");
        assert!(matches!(error, Cycle5RunRecordErrorV1::ArmBuildIdentityMismatch { .. }));
        let mut other_tree = own.clone();
        other_tree.source_tree_sha256 = "0".repeat(64);
        assert!(require_reported_identity_is_own_v1(path, &identity_bytes_v1(&other_tree)).is_err());
        assert!(require_reported_identity_is_own_v1(path, b"").is_err());
        assert!(require_reported_identity_is_own_v1(path, b"not json").is_err());
    }
}
