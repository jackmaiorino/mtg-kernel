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
use crate::native_training_store_run_v2::{
    decode_train_run_v2, refresh_derived_fields_v2, validate_train_run_record_v2,
    OpponentLadderInitializationContractV1, PopulationProgramContractV2Cycle4, TrainRunContractsV2,
    TrainRunScheduleV2, TrainRunV2, TrainerV4CandidateContractV1, ValidatedTrainRunV2,
    CYCLE4_PREREG_SHA256_V1, CYCLE4_REFRESH_INTERVAL_V1, CYCLE4_REFRESH_MANIFEST_SCHEMA_V1,
    CYCLE4_TOTAL_SUCCESSFUL_UPDATES_V1, CYCLE4_TRAINEE_START_GENERATION_V1,
    CYCLE4_TRAINEE_STOP_GENERATION_V1, CYCLE4_TRAINER_V4_BETA_F32_BITS_V1,
    CYCLE4_TRAINER_V4_CELL_CAP_V1, CYCLE4_TRAINER_V4_CONTRACT_DOCUMENT_SHA256_V1,
    CYCLE4_TRAINER_V4_LOSS_IDENTITY_V1, CYCLE4_TRAINER_V4_NUMERICAL_BACKEND_V1,
    NATIVE_TRAINING_STORE_IDENTITY_V2, TRAIN_RUN_SCHEMA_V2,
};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

/// The three arms' formal training base seeds.
///
/// Pre-registration V2 Section 8 requires the seed-schedule policy to be
/// stated explicitly and Section 9 leaves the literals to ratification; this
/// is that statement, and these three literals exist in exactly one place in
/// the tree.
///
/// Domains are disjoint by construction, on two axes:
///
/// - Between arms: each arm owns its own reserved 1,000-wide training band
///   (`[978_000, 979_000)`, `[979_000, 980_000)`, `[980_000, 981_000)`) and
///   uses that band's base. Nothing derives a training seed by adding a
///   small offset to another arm's base, so no two arms can ever draw the
///   same environment, learner or opponent seed: every seed is
///   `derive_seed(namespace, [base_seed, ...])`, keyed on the base literal.
/// - Against the payoff panels: the whole training band `[978_000, 981_000)`
///   is disjoint from the panel band `[4_100_000_000, 5_900_000_000)` the
///   wrapper strides through (three arms x 600,000,000, 32,000,000 per
///   refresh), so no training pair seed can ever collide with a panel pair
///   seed. Training and payoff seeds are therefore DISJOINT, not common.
///
/// They are also distinct from every base seed any earlier program used
/// (920012, 970001-3, 971xxx, 972002, 975002, 977002), so a cycle-4 arm can
/// never be confused with an ancestor by base seed alone.
const CYCLE4_ARM_BASE_SEED_CONTROL_R_V1: u64 = 978_000;
const CYCLE4_ARM_BASE_SEED_STATIC_RB_V1: u64 = 979_000;
const CYCLE4_ARM_BASE_SEED_TREATMENT_RB_V1: u64 = 980_000;

/// The arm's formal training base seed. See the constants above for the
/// disjoint-domain policy this implements.
#[must_use]
pub const fn cycle4_arm_base_seed_v1(arm: Cycle4ArmKindV1) -> u64 {
    match arm {
        Cycle4ArmKindV1::ControlR => CYCLE4_ARM_BASE_SEED_CONTROL_R_V1,
        Cycle4ArmKindV1::StaticRb => CYCLE4_ARM_BASE_SEED_STATIC_RB_V1,
        Cycle4ArmKindV1::TreatmentRb => CYCLE4_ARM_BASE_SEED_TREATMENT_RB_V1,
    }
}

/// One run-record build request. Every field is required; there are no
/// defaults and no environment lookups.
#[derive(Clone, Debug)]
pub struct Cycle4RunRecordRequestV1 {
    pub arm: Cycle4ArmKindV1,
    /// The cycle-3 lineage Store the arm's genesis weights come from.
    pub parent_store_root: PathBuf,
    /// The parent's generation in ITS OWN Store numbering, which for the
    /// cycle-3 focal run is also the trainee-local number: 896.
    pub parent_generation: u64,
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
    /// The pinned parent checkpoint at `--parent-generation` could not be
    /// resolved from the parent Store.
    ParentCheckpointRejected { detail: String },
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
            Self::ParentCheckpointRejected { detail } => {
                write!(formatter, "parent checkpoint rejected: {detail}")
            }
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

    let validated = assemble_cycle4_arm_run_record_v1(request.arm, &parent, initialization)?;
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
) -> Result<ValidatedTrainRunV2> {
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

    let base_seed = cycle4_arm_base_seed_v1(arm);
    let uses_v4 = arm.uses_baseline_v4_v1();
    let mut loss = parent_contracts.loss.clone();
    if uses_v4 {
        loss.identity = CYCLE4_TRAINER_V4_LOSS_IDENTITY_V1.to_owned();
    }

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
        train_step: parent_contracts.train_step.clone(),
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

    let mut record = TrainRunV2 {
        schema: TRAIN_RUN_SCHEMA_V2.to_owned(),
        store_identity: NATIVE_TRAINING_STORE_IDENTITY_V2.to_owned(),
        package: parent_record.package.clone(),
        toolchain: parent_record.toolchain.clone(),
        source: parent_record.source.clone(),
        runtime: parent_record.runtime.clone(),
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
    use crate::native_training_store_run_v2::{
        test_fixture_bytes_with_schedule_and_base_seed_ladder_init_v2,
        test_fixture_ladder_initialization_v1, test_fixture_ladder_pool_v2,
    };

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
        assemble_cycle4_arm_run_record_v1(
            arm,
            &parent_v1(),
            test_fixture_ladder_initialization_v1(),
        )
        .expect("the assembled cycle-4 arm record must validate")
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
