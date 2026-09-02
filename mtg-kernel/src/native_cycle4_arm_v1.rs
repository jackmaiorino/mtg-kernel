//! Cycle-4 arm launcher (round B).
//!
//! Contract: `docs/native_cycle4_arm_launcher_v1.md` sections 3 and 4. This
//! is the first-class, contract-validated entry point that replaces the
//! env-var `multirun_pilot_v1` test harness for the three cycle-4 arms. One
//! call runs exactly one refresh interval (128 Store generations) and
//! returns; it never loops across intervals in-process, and it never
//! evaluates.
//!
//! Generation numbering (recorded here because the contract states the
//! program in trainee-local numbers and the Store counts its own):
//!
//! - Each arm is a NEW run identity whose Store genesis is published at
//!   generation 0 with the cycle-3 g896 weights (the frozen
//!   `GenesisInitializationV2` path; there is no code path in this crate that
//!   publishes a genesis at a nonzero generation index).
//! - The arm's own Store therefore counts `0 ..= 2048`, exactly as
//!   `docs/native_population_refresh_manifest_cycle4_v1.md` states ("the
//!   arm's own store counts updates 0..=2048").
//! - Trainee-local numbering is `896 + store_generation`, so the contract's
//!   start 896 maps to store generation 0 and its stop 2944 maps to store
//!   generation 2048. The refresh manifest carries both: `program_update`
//!   IS the store generation and `trainee_local_generation` is
//!   `896 + program_update`.
//! - `--stop-generation` and [`Cycle4ArmRequestV1::stop_generation`] are
//!   STORE generations (0..=2048), never trainee-local. The launcher proves
//!   `stop_generation` names a whole interval (a multiple of 128 at or below
//!   2048) and that the Store resumes at a checkpoint-segment boundary
//!   inside `stop - 128 ..= stop` before training, so an interrupted attempt
//!   restarts against the same stop it was given.
//! - The refresh manifest labels EVERY slot generation in the contract's
//!   trainee-local numbering, including the slots bound to the arm's own run
//!   (`current-1` always, `historical-0` from refresh index 4). Translation
//!   is this launcher's job, not the manifest's: an own-run slot's
//!   `source_generation` is read from the arm's Store at
//!   `source_generation - 896` (see `store_generation_for_slot_v1`). A label
//!   below 896, a translated generation the Store does not contain, and a
//!   loaded checkpoint whose identity hashes differ from the roster's all
//!   fail closed. Slots bound to OTHER runs are read at their labels
//!   verbatim, since those runs number their own stores.
//! - The origin binding (parent run, parent checkpoint/sidecar/state
//!   SHA-256s, init generation 896) lives in the hashed run record's
//!   `contracts.opponent_ladder_initialization`, and is additionally
//!   restated in this launcher's own hashed origin record published into the
//!   chain directory at genesis.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonNullPolicyV1,
};
#[cfg(test)]
use crate::durable_move_publication_v2::ImmutableMoveMechanismV2;
use crate::durable_move_publication_v2::{
    publish_immutable_file_by_move_v2, ImmutableMovePublicationReceiptV2,
};
use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, DurableFileExpectationV1,
};
use crate::native_baseline_checkpoint_chain_v4::{
    manifest_final_name_v4, parse_sidecar_record_name_v4, publish_baseline_record_v4,
    record_final_name_v4, resume_baseline_chain_v4, sidecar_record_name_v4,
    BaselineChainRecordPartsV4, BaselineChainResumeVerdictV4,
};
use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
use crate::native_ladder_pool_resolution_v1::{
    ladder_init_as_checkpoint_ref_v1, resolve_ladder_checkpoint_authority_v1,
    stage_ladder_checkpoint_initialization_v1,
};
use crate::native_policy_baseline_state_v4::{BaselineObservationV4, NativeBaselineStateV4};
use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
use crate::native_population_opponent_v1::{
    PopulationOpponentEngineV1, PopulationSlotOccupantV1, PopulationWeightVectorV1,
    POPULATION_OPPONENT_SLOT_COUNT_V1,
};
use crate::native_population_refresh_builder_cycle4_v1::{
    cycle4_chain_manifest_filename_v1, cycle4_chain_panel_filename_v1,
};
use crate::native_population_refresh_manifest_cycle4_v1::{
    decode_cycle4_refresh_manifest_v1, Cycle4RefreshManifestV1, Cycle4RefreshSlotV1,
    CYCLE4_REFRESH_INTERVAL_V1, CYCLE4_REFRESH_MAX_INDEX_V1, CYCLE4_SLOT_COUNT_V1,
    CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1,
};
use crate::native_training_executor_v1::NativeTrainingExecutionConfigV1;
use crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2;
use crate::native_training_store_boundary_v2::build_genesis_native_training_boundary_v2;
use crate::native_training_store_checkpoint_v3::{
    build_genesis_checkpoint_manifest_v2_v3, derive_genesis_weights_only_payload_v2_v3,
    CheckpointManifestV3,
};
use crate::native_training_store_checkpoint_v4::{
    checkpoint_manifest_parts_v4_from_v3, decode_checkpoint_manifest_v4,
};
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
use crate::native_training_store_prepared_segment_v2::{
    prepare_segment_baseline_v4_v2, prepare_segment_v2,
};
use crate::native_training_store_reference_latest_v2::{
    build_checkpoint_reference_v2, build_latest_v2,
};
use crate::native_training_store_resume_v2::{
    load_native_training_boundary_baseline_v4_v2, load_native_training_boundary_v2,
    peek_latest_generation_index_from_store_v2,
};
use crate::native_training_store_resume_v2::{
    resume_native_training_store_with_session_baseline_v4_v2,
    resume_native_training_store_with_session_v2, validate_native_training_store_baseline_v4_v2,
    validate_native_training_store_v2, NativeTrainingStoreContinuationSessionV2,
    NativeTrainingStoreResumeV2,
};
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::{
    decode_train_run_v2, TrainerLossIdentityV2, ValidatedTrainRunV2,
};
use crate::native_training_store_segment_manifest_v2::build_genesis_segment_manifest_v2;
use crate::native_training_store_update_group_v4::{
    decode_update_baseline_record_v4, BaselineChainAccessV4, BaselineSidecarSourceV4,
};
use crate::native_training_store_v2::{
    publish_genesis_generation_v2, publish_prepared_segment_with_session_baseline_v4_v2,
    publish_prepared_segment_with_session_v2,
};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Machine-local mapping from each pinned slot identity to the absolute
/// store root that holds it. Deliberately a SEPARATE schema from the payoff
/// panel runner's `mtg-kernel-cycle4-slot-locator/v1` (which is index-keyed):
/// this one is identity-keyed so a wrong store cannot occupy a right slot,
/// and the launcher cross-checks the whole key set against the manifest's
/// roster before any store is opened. Absolute paths never enter a hashed
/// contract, which is exactly why this file exists.
pub const CYCLE4_ARM_SLOT_LOCATOR_SCHEMA_V1: &str = "mtg-kernel-cycle4-arm-slot-locator/v1";

/// Launcher-level hashed record binding the arm's genesis origin: the parent
/// checkpoint the g896 state came from, the init generation, and the arm's
/// own run identity. Published once, atomically, into the chain directory
/// when the arm's Store genesis is authored.
pub const CYCLE4_ARM_ORIGIN_RECORD_SCHEMA_V1: &str = "mtg-kernel-cycle4-arm-origin/v1";

/// Fixed on-disk name of the origin record inside the chain directory.
pub const CYCLE4_ARM_ORIGIN_RECORD_FILENAME_V1: &str = "arm-origin.record.json";

/// Launcher-level marker pinning one Store prefix to one mode for the life of
/// that prefix. It lives in the Store root's PARENT directory (the "Store
/// prefix"), never inside the Store: the Store's own leaf grammar is closed
/// and a stray file under the root would be a layout violation.
///
/// The marker exists for one reason: `--preflight-updates` relaxes the
/// interval check from the pre-registered 128 to a short window, so a Store a
/// preflight ever TRAINED cannot become a formal artifact, and a formal Store
/// cannot be re-entered under the relaxed check. See
/// [`Cycle4ArmStoreModeV1`] for the one admissible transition.
pub const CYCLE4_ARM_MODE_MARKER_SCHEMA_V1: &str = "mtg-kernel-cycle4-arm-mode-marker/v1";

/// Fixed on-disk name of the mode marker inside the Store prefix.
pub const CYCLE4_ARM_MODE_MARKER_FILENAME_V1: &str = "cycle4-arm-mode.marker.json";

/// Largest `--preflight-updates` window. The preflight ladder only ever needs
/// a couple of updates per prefix; bounding it here keeps a mistyped flag from
/// quietly becoming a long relaxed-interval run.
pub const CYCLE4_ARM_PREFLIGHT_MAX_UPDATES_V1: u64 = 8;

/// Total Store generations the whole cycle-4 program runs (16 intervals of
/// 128), i.e. trainee-local 896 through 2944.
const CYCLE4_ARM_STORE_GENERATION_TOTAL_V1: u64 =
    CYCLE4_REFRESH_MAX_INDEX_V1 * CYCLE4_REFRESH_INTERVAL_V1;

/// Which arm this process runs. The value must equal the arm the run record
/// itself declares; the launcher never infers one from the other.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cycle4ArmKindV1 {
    ControlR,
    StaticRb,
    TreatmentRb,
}

impl Cycle4ArmKindV1 {
    #[must_use]
    pub fn from_wire_v1(value: &str) -> Option<Self> {
        match value {
            "control-r" => Some(Self::ControlR),
            "static-rb" => Some(Self::StaticRb),
            "treatment-rb" => Some(Self::TreatmentRb),
            _ => None,
        }
    }

    #[must_use]
    pub const fn wire_v1(self) -> &'static str {
        match self {
            Self::ControlR => "control-r",
            Self::StaticRb => "static-rb",
            Self::TreatmentRb => "treatment-rb",
        }
    }

    /// TREATMENT-RB and STATIC-RB run `terminal_reinforce_value/v4-candidate`
    /// and therefore carry a baseline chain; CONTROL-R never installs a
    /// baseline and runs the frozen v3 path bit-identically.
    #[must_use]
    pub const fn uses_baseline_v4_v1(self) -> bool {
        match self {
            Self::ControlR => false,
            Self::StaticRb | Self::TreatmentRb => true,
        }
    }

    /// STATIC-RB's manifest never advances past genesis.
    #[must_use]
    pub const fn static_pool_v1(self) -> bool {
        matches!(self, Self::StaticRb)
    }
}

/// One interval's complete, typed request. Every path is machine-local and
/// never enters a hashed artifact.
#[derive(Clone, Debug)]
pub struct Cycle4ArmRequestV1 {
    pub arm: Cycle4ArmKindV1,
    /// The Store root directory itself (its parent and basename are derived).
    pub store_root: PathBuf,
    /// The arm's formal `run.json` bytes on disk.
    pub run_record: PathBuf,
    /// The arm's baseline chain directory (boundary records, per-update
    /// sidecars, and the origin record).
    pub chain_dir: PathBuf,
    /// This interval's cycle-4 refresh manifest. Its own directory is the
    /// refresh chain directory: predecessors are read from it by the pinned
    /// `refresh-NN.manifest.json` / `refresh-NN.panel.json` naming scheme.
    pub refresh_manifest: PathBuf,
    /// The panel bytes the manifest binds by hash. Absent only for genesis
    /// (`refresh_index == 0`).
    pub payoff_panel: Option<PathBuf>,
    /// Identity-keyed slot locator, see
    /// [`CYCLE4_ARM_SLOT_LOCATOR_SCHEMA_V1`].
    pub slot_locator: PathBuf,
    /// STORE generation this process stops at: the end of the interval the
    /// manifest opens, a multiple of 128 at or below 2048. The Store may
    /// resume anywhere inside that interval (an interrupted attempt keeps its
    /// original stop), or at the stop itself when it already completed.
    pub stop_generation: u64,
    /// Bounded preflight provision (`docs/native_cycle4_arm_launcher_v1.md`
    /// Section 6's CONTROL preflight ladder). `None` is the formal path and
    /// is byte-for-byte the pre-registered behavior. `Some(n)` relaxes the
    /// interval check to `stop == resume + n` for `n` in
    /// `1 ..= CYCLE4_ARM_PREFLIGHT_MAX_UPDATES_V1`, and can only ever run
    /// against a throwaway Store prefix: the mode marker refuses a prefix
    /// that a formal run already claimed, and refuses to let a formal run
    /// re-enter a prefix a preflight claimed.
    pub preflight_updates: Option<u64>,
}

/// What one interval actually did.
#[derive(Clone, Debug)]
pub struct Cycle4ArmOutcomeV1 {
    pub arm: Cycle4ArmKindV1,
    pub resume_generation_index: u64,
    pub latest_generation_index: u64,
    pub trainee_local_generation: u64,
    pub refresh_index: u64,
    pub refresh_manifest_sha256: String,
    /// Newest baseline chain boundary generation after this interval, or
    /// `None` for CONTROL-R (which has no chain).
    pub baseline_chain_generation: Option<u64>,
}

/// How the bin must exit: a contract rejection is 3, a runtime failure is 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cycle4ArmFailureV1 {
    Contract,
    Runtime,
}

#[derive(Clone, Debug)]
pub struct Cycle4ArmErrorV1 {
    failure: Cycle4ArmFailureV1,
    code: &'static str,
    detail: String,
}

impl Cycle4ArmErrorV1 {
    fn contract(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            failure: Cycle4ArmFailureV1::Contract,
            code,
            detail: detail.into(),
        }
    }

    fn runtime(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            failure: Cycle4ArmFailureV1::Runtime,
            code,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn failure_v1(&self) -> Cycle4ArmFailureV1 {
        self.failure
    }

    #[must_use]
    pub const fn code_v1(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn detail_v1(&self) -> &str {
        &self.detail
    }

    /// 3 for a contract rejection, 1 for a runtime failure
    /// (`docs/native_cycle4_arm_launcher_v1.md` Section 4).
    #[must_use]
    pub const fn exit_code_v1(&self) -> i32 {
        match self.failure {
            Cycle4ArmFailureV1::Contract => 3,
            Cycle4ArmFailureV1::Runtime => 1,
        }
    }
}

impl Display for Cycle4ArmErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for Cycle4ArmErrorV1 {}

type Result<T> = std::result::Result<T, Cycle4ArmErrorV1>;

// ---------------------------------------------------------------------
// Slot locator
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle4SlotLocatorEntryV1 {
    checkpoint_manifest_sha256: String,
    store_root: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle4SlotLocatorV1 {
    schema: String,
    stores: Vec<Cycle4SlotLocatorEntryV1>,
    /// Absolute path to the parent (cycle-3 lineage) Store the arm's genesis
    /// weights are copied from. Required only when the arm's Store has no
    /// genesis yet; ignored on every later interval. Lives here rather than
    /// as a launcher flag so the contract's closed flag list stays closed and
    /// no absolute path enters a hashed artifact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    genesis_parent_store_root: Option<String>,
}

fn is_lower_hex_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn decode_slot_locator_v1(bytes: &[u8]) -> Result<Cycle4SlotLocatorV1> {
    let locator: Cycle4SlotLocatorV1 = serde_json::from_slice(bytes).map_err(|error| {
        Cycle4ArmErrorV1::contract("cycle4_arm_v1_slot_locator_malformed", error.to_string())
    })?;
    if locator.schema != CYCLE4_ARM_SLOT_LOCATOR_SCHEMA_V1 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_slot_locator_schema",
            locator.schema,
        ));
    }
    if locator.stores.len() != CYCLE4_SLOT_COUNT_V1 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_slot_locator_slot_count",
            format!("{} entries", locator.stores.len()),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &locator.stores {
        if !is_lower_hex_sha256_v1(&entry.checkpoint_manifest_sha256) {
            return Err(Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_slot_locator_identity",
                entry.checkpoint_manifest_sha256.clone(),
            ));
        }
        if !seen.insert(entry.checkpoint_manifest_sha256.clone()) {
            return Err(Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_slot_locator_duplicate_identity",
                entry.checkpoint_manifest_sha256.clone(),
            ));
        }
        if entry.store_root.is_empty() || !Path::new(&entry.store_root).is_absolute() {
            return Err(Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_slot_locator_relative_path",
                entry.store_root.clone(),
            ));
        }
    }
    if let Some(parent) = &locator.genesis_parent_store_root {
        if parent.is_empty() || !Path::new(parent).is_absolute() {
            return Err(Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_slot_locator_relative_path",
                parent.clone(),
            ));
        }
    }
    Ok(locator)
}

/// Orders the locator's absolute paths by the manifest's own roster. The
/// locator's identity set must equal the roster's identity set exactly: a
/// missing, extra, or substituted store fails closed here, before any Store
/// is opened.
fn slot_store_roots_for_manifest_v1(
    locator: &Cycle4SlotLocatorV1,
    manifest: &Cycle4RefreshManifestV1,
) -> Result<Vec<PathBuf>> {
    let mut by_identity: BTreeMap<&str, &str> = BTreeMap::new();
    for entry in &locator.stores {
        by_identity.insert(
            entry.checkpoint_manifest_sha256.as_str(),
            entry.store_root.as_str(),
        );
    }
    let slots = manifest.slots_v1();
    if slots.len() != CYCLE4_SLOT_COUNT_V1 || by_identity.len() != CYCLE4_SLOT_COUNT_V1 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_slot_locator_slot_count",
            "roster and locator must both carry exactly eight slots",
        ));
    }
    let mut roots = Vec::with_capacity(CYCLE4_SLOT_COUNT_V1);
    for slot in slots {
        let root = by_identity
            .remove(slot.checkpoint_manifest_sha256.as_str())
            .ok_or_else(|| {
                Cycle4ArmErrorV1::contract(
                    "cycle4_arm_v1_slot_locator_roster_mismatch",
                    format!(
                        "slot {} identity {} is not in the locator",
                        slot.slot_index, slot.checkpoint_manifest_sha256
                    ),
                )
            })?;
        roots.push(PathBuf::from(root));
    }
    if !by_identity.is_empty() {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_slot_locator_roster_mismatch",
            "locator carries an identity the roster does not",
        ));
    }
    Ok(roots)
}

// ---------------------------------------------------------------------
// Cycle-4 population resolution (sibling of resolve_population_opponent_v1)
// ---------------------------------------------------------------------

/// The Store generation one roster slot is actually read at.
///
/// The refresh manifest labels every slot in the contract's trainee-local
/// numbering (`docs/native_population_refresh_manifest_cycle4_v1.md`,
/// Frame). For a slot bound to the ARM'S OWN run that label is 896 above the
/// arm's Store numbering, because the arm is a new run identity seeded from
/// the cycle-3 g896 checkpoint and its Store publishes genesis at generation
/// 0 (`0 ..= 2048` for `896 ..= 2944`). Translation lives here rather than
/// in the manifest so every identity in the roster keeps one consistent
/// lineage numbering.
///
/// Fails closed on an own-run label below 896: there is no Store generation
/// such a label could name. Slots bound to other runs are returned
/// unchanged; those runs number their own Stores.
fn store_generation_for_slot_v1(slot: &Cycle4RefreshSlotV1, arm_run_sha256: &str) -> Result<u64> {
    if slot.source_run_sha256 != arm_run_sha256 {
        return Ok(slot.source_generation);
    }
    slot.source_generation
        .checked_sub(CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1)
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_own_run_slot_generation",
                format!(
                    "slot {} names the arm's own run at trainee-local generation {}, which is below the program start {CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1}",
                    slot.slot_index, slot.source_generation
                ),
            )
        })
}

/// Cycle-4 sibling of `native_population_runtime_resolution_v1`'s
/// `resolve_population_opponent_v1`: same shape (reopen each slot's own Store
/// through its own `run.json` and complete walk, re-verify every declared
/// identity hash against the actually loaded artifact, then build the eight
/// immutable inference handles), retyped onto the cycle-4 slot record's
/// five-hash identity. Cycle-4 admits no search occupants, so every slot is
/// Store-backed.
/// From refresh index 1 the roster's `current-1` slot (and from index 4 its
/// `historical-0` slot) binds the ARM'S OWN run, whose Store is a
/// `trainer_v4_candidate` Store for the two rb arms. Such a Store's evidence
/// only validates through the v4 recompute, so those slots resolve through
/// `access`; every frozen slot is a v3 Store and resolves through the plain
/// path, byte for byte as before.
///
/// Own-run slots additionally translate their trainee-local
/// `source_generation` label into the arm's Store numbering
/// (`store_generation_for_slot_v1`) before the boundary is opened, and the
/// loaded checkpoint's `generation_index` is compared against the TRANSLATED
/// value; other-run slots keep their labels verbatim.
fn resolve_population_opponent_cycle4_v1(
    manifest: &Cycle4RefreshManifestV1,
    slot_store_roots: &[PathBuf],
    arm_run_sha256: &str,
    access: Option<&dyn BaselineChainAccessV4>,
) -> Result<PopulationOpponentEngineV1> {
    if slot_store_roots.len() != POPULATION_OPPONENT_SLOT_COUNT_V1
        || manifest.slots_v1().len() != POPULATION_OPPONENT_SLOT_COUNT_V1
    {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_population_slot_count",
            "eight slots required",
        ));
    }
    // Translate every own-run label into its Store generation FIRST, before
    // any file is opened: a label that cannot name a Store generation at all
    // is rejected without touching the filesystem.
    let store_generations = manifest
        .slots_v1()
        .iter()
        .map(|slot| store_generation_for_slot_v1(slot, arm_run_sha256))
        .collect::<Result<Vec<u64>>>()?;
    let mut handles = Vec::with_capacity(POPULATION_OPPONENT_SLOT_COUNT_V1);
    for ((slot, store_root), store_generation) in manifest
        .slots_v1()
        .iter()
        .zip(slot_store_roots)
        .zip(store_generations)
    {
        let mismatch = |detail: String| {
            Cycle4ArmErrorV1::contract("cycle4_arm_v1_population_authority_mismatch", detail)
        };
        let run_bytes = std::fs::read(store_root.join("run.json")).map_err(|error| {
            Cycle4ArmErrorV1::runtime(
                "cycle4_arm_v1_population_run_read",
                format!("{}: {error}", store_root.display()),
            )
        })?;
        let slot_run = decode_train_run_v2(&run_bytes)
            .map_err(|error| mismatch(format!("{} run.json: {error}", store_root.display())))?;
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root).map_err(|error| {
            Cycle4ArmErrorV1::runtime(
                "cycle4_arm_v1_population_root_open",
                format!("{}: {error}", store_root.display()),
            )
        })?;
        let is_own_run = slot.source_run_sha256 == arm_run_sha256;
        let boundary = match (is_own_run, access) {
            (true, Some(access)) => load_native_training_boundary_baseline_v4_v2(
                &root,
                &slot_run,
                store_generation,
                access,
            ),
            _ => load_native_training_boundary_v2(&root, &slot_run, store_generation),
        }
        .map_err(|error| {
            mismatch(format!(
                "{} store generation {store_generation} (slot label {}): {error}",
                store_root.display(),
                slot.source_generation
            ))
        })?;
        let checkpoint = boundary.checkpoint();
        let matches_authority = slot_run.run_sha256() == slot.source_run_sha256
            && slot_run.record().schedule.base_seed == slot.source_base_seed
            && checkpoint.generation_index() == store_generation
            && lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256())
                == slot.checkpoint_manifest_sha256
            && lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256())
                == slot.checkpoint_payload_sha256
            && lower_hex_raw32_v1(checkpoint.model_parameter_sha256())
                == slot.model_parameter_sha256
            && lower_hex_raw32_v1(checkpoint.train_state_sha256()) == slot.train_state_sha256;
        if !matches_authority {
            return Err(mismatch(format!(
                "slot {} at {}",
                slot.slot_index,
                store_root.display()
            )));
        }
        let inference =
            load_native_checkpoint_inference_v1(&slot_run, checkpoint, boundary.payload())
                .map_err(|error| {
                    mismatch(format!("slot {} inference: {error}", slot.slot_index))
                })?;
        handles.push(PopulationSlotOccupantV1::Checkpoint(inference));
    }
    let handles: [PopulationSlotOccupantV1; POPULATION_OPPONENT_SLOT_COUNT_V1] =
        handles.try_into().map_err(|_| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_population_slot_count",
                "eight handles required",
            )
        })?;
    let weight_units: [u64; POPULATION_OPPONENT_SLOT_COUNT_V1] =
        std::array::from_fn(|index| manifest.slots_v1()[index].weight_units);
    let total = weight_units
        .iter()
        .try_fold(0_u64, |sum, weight| sum.checked_add(*weight))
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract("cycle4_arm_v1_population_weight", "weight total overflow")
        })?;
    let weights = PopulationWeightVectorV1::new_v1(weight_units, total).map_err(|error| {
        Cycle4ArmErrorV1::contract("cycle4_arm_v1_population_weight", error.to_string())
    })?;
    Ok(PopulationOpponentEngineV1::new_v1(weights, handles))
}

// ---------------------------------------------------------------------
// Head-to-head boundary mode (payoff panel probe)
// ---------------------------------------------------------------------

/// How one side of a head-to-head evaluation loads its checkpoint boundary.
///
/// Test-only because its one consumer, the payoff panel's own
/// `ladder_head_to_head_eval_v1` probe, is an ignored test the panel runner
/// drives as a subprocess; nothing in the shipped library loads a
/// head-to-head side.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cycle4H2hBoundaryModeV1 {
    /// `load_native_training_boundary_v2`: the frozen v3 walk.
    Plain,
    /// `load_native_training_boundary_baseline_v4_v2` against the side's own
    /// chain directory.
    BaselineV4,
}

/// Decides how one head-to-head side must load its boundary, from whether
/// its run declares the v4 trainer and whether a chain directory was
/// supplied for it (review finding P1).
///
/// A `trainer_v4_candidate` Store that has trained past genesis carries v4
/// update evidence, which the plain v3 walk rejects outright: without a
/// chain directory such a side cannot be loaded at all, so requiring one is
/// the difference between a clear rejection and a panic mid-panel. The
/// converse is equally load-bearing: a chain directory handed to a v3 run
/// would be silently ignored, so it is refused rather than accepted as a
/// no-op.
#[cfg(test)]
pub(crate) fn cycle4_h2h_boundary_mode_v1(
    declares_trainer_v4: bool,
    chain_dir: Option<&str>,
) -> std::result::Result<Cycle4H2hBoundaryModeV1, &'static str> {
    match (declares_trainer_v4, chain_dir) {
        (true, Some(_)) => Ok(Cycle4H2hBoundaryModeV1::BaselineV4),
        (false, None) => Ok(Cycle4H2hBoundaryModeV1::Plain),
        (true, None) => Err("cycle4_h2h_v4_run_requires_chain_dir"),
        (false, Some(_)) => Err("cycle4_h2h_chain_dir_requires_v4_run"),
    }
}

// ---------------------------------------------------------------------
// Baseline chain directory access
// ---------------------------------------------------------------------

/// Filesystem-backed [`BaselineChainAccessV4`] over one arm's chain
/// directory. Two namespaces share the directory by contract: this chain's
/// per-boundary records/manifests (`baseline-<8 digits>.*`) and the
/// launcher's per-update sidecars (`baseline-update-<8 digits>.record.json`).
///
/// Boundary states resolve in one of three ways, in order: generation 0 is
/// the empty state; a generation with a chain record decodes that record's
/// v4 manifest against the Store's own core train-state hash for that
/// generation; and a generation at most ONE checkpoint boundary past the
/// chain tip is reconstructed by replaying that boundary's per-update
/// sidecars, which is exactly the crash window the chain's
/// `StoreAheadByOneBoundary` verdict admits. Anything further ahead fails
/// closed.
pub(crate) struct Cycle4BaselineChainAccessV1 {
    chain_dir: PathBuf,
    checkpoint_segment_updates: u64,
    /// Store core train-state hashes observed from validated checkpoints,
    /// the only admissible input to a chain manifest decode.
    observed_core_hashes: RefCell<BTreeMap<u64, [u8; 32]>>,
    /// Memoized boundary states. Resolution is a pure function of the chain
    /// directory plus observed hashes, so caching cannot change a verdict;
    /// it only stops the publisher's repeated revalidation passes from
    /// re-reading the same records.
    boundary_states: RefCell<BTreeMap<u64, NativeBaselineStateV4>>,
    /// The update index whose sidecar this open is allowed to reconstruct
    /// from its own committed evidence, set by `reconcile_staged_sidecars_v1`
    /// to the Store's tip update. `None` forbids reconstruction entirely,
    /// which is the state every path that has not reconciled is left in.
    reconstructable_tip_update: Cell<Option<u64>>,
}

/// Subdirectory of the chain directory holding sidecars that are staged but
/// not yet promoted. A third namespace beside the chain's own boundary
/// records and the promoted per-update sidecars, and a DIRECTORY rather than
/// a name prefix so the chain's record listing (which skips non-files) can
/// never mistake a staged record for a boundary record.
/// Reserved suffix marking a sidecar record that is staged but not yet
/// promoted, appended to that record's own final name.
///
/// Staging lives IN the chain directory under this grammar rather than in a
/// `baseline-staged/` subdirectory (review finding P1). A subdirectory
/// created fresh on the first sidecar of a run is itself a new entry in the
/// chain directory, and the durable move primitive syncs files inside a
/// parent, never the parent's own entry one level up, so a crash after the
/// Store commit could lose the whole staging directory and with it every
/// unpromoted record. Naming staged records inside the directory that
/// already holds the chain's own boundary records removes that level
/// entirely: a staged record is exactly as durable as the chain records
/// beside it, with no new directory entry to lose and no new unsafe
/// directory-flush code. `list_record_generations_v4` tolerates the grammar
/// (the suffix defeats its `.record.json` match, so the name is skipped, and
/// the primitive's own dot-prefixed staging file defeats its `baseline-`
/// match), which is the property that makes sharing the directory legal.
const CYCLE4_STAGED_SIDECAR_SUFFIX_V1: &str = ".staged-cycle4-v1";

/// Directory the PREVIOUS staging layout used. Read-only from here: a Store
/// left mid-segment by that layout still has records and debris in it, so
/// reconciliation drains it and then removes it, and nothing is ever written
/// there again.
const CYCLE4_LEGACY_STAGED_SIDECAR_DIRNAME_V1: &str = "baseline-staged";

/// Legacy temporary-file infix `write_file_atomically_v1` used before staged
/// records became durable publications: `<final name>.tmp-<pid>`.
const CYCLE4_LEGACY_TEMPORARY_INFIX_V1: &str = ".tmp-";

/// Staged name for one sidecar record's final name.
fn cycle4_staged_sidecar_name_v1(final_name: &str) -> String {
    format!("{final_name}{CYCLE4_STAGED_SIDECAR_SUFFIX_V1}")
}

/// Staging-file name the durable move primitive writes before moving a file
/// onto `final_name`, mirroring the chain module's own `stage_name_v4`
/// shape: dot-prefixed, so it can never be mistaken for a record.
fn cycle4_sidecar_stage_name_v1(final_name: &str) -> String {
    format!(".{final_name}.stage-cycle4-v1")
}

/// What one directory entry is, as far as the staging path is concerned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cycle4StagedEntryV1 {
    /// A staged sidecar record for this update index.
    Record(u64),
    /// Removable debris from an interrupted publication. Exactly two
    /// grammars qualify (review finding P2): the legacy
    /// `<sidecar name>.tmp-<pid>` temporary the pre-durable writer left, and
    /// the durable primitive's own `.<name>.stage-cycle4-v1` staging file.
    /// Both are recognized ONLY when what they decorate is itself a sidecar
    /// or staged-sidecar name, so debris belonging to another writer in the
    /// shared chain directory is never swept up.
    Debris,
    /// Nothing to do with the staging grammar.
    Foreign,
}

/// Classifies one entry name against the staging grammar.
///
/// `Foreign` is the honest answer for the chain directory, which several
/// writers share; each namespace owner fails closed on its OWN grammar. A
/// name that is in this grammar but malformed is the one case that must fail
/// closed, and it is reported by returning `None`.
fn classify_staged_entry_v1(name: &str) -> Option<Cycle4StagedEntryV1> {
    // The primitive's staging file, decorating a staged record name.
    if let Some(inner) = name
        .strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(".stage-cycle4-v1"))
    {
        return Some(if is_sidecar_or_staged_sidecar_name_v1(inner) {
            Cycle4StagedEntryV1::Debris
        } else {
            Cycle4StagedEntryV1::Foreign
        });
    }
    // The legacy temporary, decorating a sidecar or staged-sidecar name.
    if let Some((base, pid)) = name.rsplit_once(CYCLE4_LEGACY_TEMPORARY_INFIX_V1) {
        if !pid.is_empty()
            && pid.bytes().all(|byte| byte.is_ascii_digit())
            && is_sidecar_or_staged_sidecar_name_v1(base)
        {
            return Some(Cycle4StagedEntryV1::Debris);
        }
        return Some(Cycle4StagedEntryV1::Foreign);
    }
    // A staged record.
    if let Some(base) = name.strip_suffix(CYCLE4_STAGED_SIDECAR_SUFFIX_V1) {
        // In this grammar but not a record name: the one fail-closed case.
        return parse_sidecar_record_name_v4(base).map(Cycle4StagedEntryV1::Record);
    }
    Some(Cycle4StagedEntryV1::Foreign)
}

/// Whether `path` is a REGULAR file, never following a final symlink.
///
/// `Path::is_file` follows, so a reserved name that is a symlink would read
/// bytes from outside the chain directory (review finding P2). Every staging
/// path decision goes through this instead.
fn is_regular_file_v1(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

/// One staged record, together with every copy of it that must drain when it
/// is promoted.
///
/// Two layouts can hold the same update: the current reserved suffix in the
/// chain directory, and the previous `baseline-staged/` directory. Both are
/// read, and a disagreement between them is a hard stop rather than a silent
/// preference for either (review finding P2).
#[derive(Debug)]
struct Cycle4StagedRecordV1 {
    bytes: Vec<u8>,
    copies: Vec<PathBuf>,
}

fn is_sidecar_or_staged_sidecar_name_v1(name: &str) -> bool {
    let base = name
        .strip_suffix(CYCLE4_STAGED_SIDECAR_SUFFIX_V1)
        .unwrap_or(name);
    parse_sidecar_record_name_v4(base).is_some()
}

impl Cycle4BaselineChainAccessV1 {
    /// Also the payoff panel probe's constructor: an evaluator opens a slot
    /// Store read-only, so it never reconciles and therefore never
    /// reconstructs (`may_reconstruct_sidecar_v4` stays false), which keeps
    /// a missing sidecar a hard failure on that path.
    pub(crate) fn new_v1(chain_dir: PathBuf, checkpoint_segment_updates: u64) -> Self {
        Self {
            chain_dir,
            checkpoint_segment_updates,
            observed_core_hashes: RefCell::new(BTreeMap::new()),
            boundary_states: RefCell::new(BTreeMap::new()),
            reconstructable_tip_update: Cell::new(None),
        }
    }

    fn legacy_staged_dir_v1(&self) -> PathBuf {
        self.chain_dir.join(CYCLE4_LEGACY_STAGED_SIDECAR_DIRNAME_V1)
    }

    /// Publishes one sidecar's exact bytes at `final_name` under `parent_dir`
    /// through the repository's durable move primitive (review finding P1).
    ///
    /// `write_file_atomically_v1` syncs the staging file but then does a
    /// plain `rename`, which leaves the DIRENT unsynced: after a host crash
    /// or reboot the name can be gone even though the Store commit that
    /// followed it survived, and reconciliation reconstructs only the tip, so
    /// an earlier missing sidecar leaves a v4 Store unresumable.
    /// `publish_immutable_file_by_move_v2` is exactly
    /// `MoveFileExW(.., MOVEFILE_WRITE_THROUGH)` on Windows and a
    /// no-replace rename followed by a directory `sync_all` elsewhere, and it
    /// is the same primitive the chain's own boundary records use.
    ///
    /// Create-new by construction: the caller has already proven the final
    /// name is absent, and a stale staging file from an interrupted attempt
    /// is removed first so a replay cannot collide with its own debris.
    fn publish_sidecar_bytes_v1(
        parent_dir: &Path,
        final_name: &str,
        bytes: &[u8],
    ) -> Result<ImmutableMovePublicationReceiptV2> {
        let publication_error = |detail: String| {
            Cycle4ArmErrorV1::runtime("cycle4_arm_v1_baseline_sidecar_publish", detail)
        };
        std::fs::create_dir_all(parent_dir)
            .map_err(|error| publication_error(format!("{}: {error}", parent_dir.display())))?;
        let stage_name = cycle4_sidecar_stage_name_v1(final_name);
        match std::fs::remove_file(parent_dir.join(&stage_name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(publication_error(format!(
                    "{}: {error}",
                    parent_dir.join(&stage_name).display()
                )))
            }
        }
        let parent = capture_existing_publication_parent_v1(parent_dir)
            .map_err(|error| publication_error(format!("{}: {error}", parent_dir.display())))?;
        let expectation = DurableFileExpectationV1::from_bytes(bytes)
            .map_err(|error| publication_error(error.to_string()))?;
        publish_immutable_file_by_move_v2(&parent, &stage_name, final_name, bytes, expectation)
            .map_err(|error| publication_error(format!("{final_name}: {error}")))
    }

    fn staged_sidecar_path_v1(&self, update_index: u64) -> Option<PathBuf> {
        sidecar_record_name_v4(update_index)
            .ok()
            .map(|name| self.chain_dir.join(cycle4_staged_sidecar_name_v1(&name)))
    }

    /// Where the previous layout would have staged this update.
    fn legacy_staged_sidecar_path_v1(&self, update_index: u64) -> Option<PathBuf> {
        sidecar_record_name_v4(update_index)
            .ok()
            .map(|name| self.legacy_staged_dir_v1().join(name))
    }

    /// Every regular-file copy of one update's staged record, current layout
    /// first. No-follow throughout, so a symlink under a reserved name is
    /// never a copy.
    fn staged_sidecar_copies_v1(&self, update_index: u64) -> Vec<PathBuf> {
        [
            self.staged_sidecar_path_v1(update_index),
            self.legacy_staged_sidecar_path_v1(update_index),
        ]
        .into_iter()
        .flatten()
        .filter(|path| is_regular_file_v1(path))
        .collect()
    }

    /// The staged record for one update, read from every layout that holds
    /// it.
    ///
    /// Deduplicating the two layouts by update index used to hide a
    /// disagreement between them: the current copy was promoted, the legacy
    /// copy was left unread and unremoved, and conflicting bytes could
    /// therefore advance the chain once before a later commit tripped over
    /// the leftover. Both copies are now read; identical bytes promote once
    /// and drain both, and different bytes fail closed here, before any
    /// promotion (review finding P2).
    fn resolve_staged_record_v1(&self, update_index: u64) -> Result<Option<Cycle4StagedRecordV1>> {
        let copies = self.staged_sidecar_copies_v1(update_index);
        if copies.is_empty() {
            return Ok(None);
        }
        let mut bytes: Option<Vec<u8>> = None;
        for path in &copies {
            let read = std::fs::read(path).map_err(|error| {
                Cycle4ArmErrorV1::runtime(
                    "cycle4_arm_v1_baseline_sidecar_staging",
                    format!("{}: {error}", path.display()),
                )
            })?;
            match &bytes {
                None => bytes = Some(read),
                Some(first) if *first == read => {}
                Some(_) => {
                    return Err(Cycle4ArmErrorV1::contract(
                        "cycle4_arm_v1_baseline_sidecar_layout_conflict",
                        format!(
                            "update {update_index} is staged under two layouts with different bytes: {}",
                            copies
                                .iter()
                                .map(|path| path.display().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    ))
                }
            }
        }
        Ok(bytes.map(|bytes| Cycle4StagedRecordV1 { bytes, copies }))
    }

    /// Scans one directory for staging-grammar entries.
    ///
    /// `exclusive` says whether this process owns every name in it: the
    /// legacy staging directory is ours alone, so a foreign name there fails
    /// closed, while the chain directory is shared with the chain's own
    /// records, the origin record, and the genesis authority, so a foreign
    /// name there is simply not ours. Either way a name that IS in the
    /// staging grammar but malformed fails closed, and recognized debris is
    /// reported for removal rather than tripping the scan.
    fn scan_staging_entries_v1(dir: &Path, exclusive: bool) -> Result<(Vec<u64>, Vec<PathBuf>)> {
        let staging_error = |detail: String| {
            Cycle4ArmErrorV1::runtime("cycle4_arm_v1_baseline_sidecar_staging", detail)
        };
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), Vec::new()))
            }
            Err(error) => return Err(staging_error(format!("{}: {error}", dir.display()))),
        };
        let mut records = Vec::new();
        let mut debris = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|error| staging_error(format!("{}: {error}", dir.display())))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                if exclusive {
                    return Err(Cycle4ArmErrorV1::contract(
                        "cycle4_arm_v1_baseline_sidecar_staging",
                        format!("{}: a non-UTF-8 name is not a staged record", dir.display()),
                    ));
                }
                continue;
            };
            // The legacy layout staged records under their bare final names;
            // the current one appends the reserved suffix. Both are read.
            let classified = if exclusive {
                parse_sidecar_record_name_v4(name)
                    .map(Cycle4StagedEntryV1::Record)
                    .or_else(|| classify_staged_entry_v1(name))
            } else {
                classify_staged_entry_v1(name)
            };
            // Classified BEFORE the file-type gate, and the gate is
            // no-follow (review finding P2): a reserved name that is a
            // symlink, a directory, or any other non-regular entry used to be
            // skipped here and then followed by the lookup, which would let a
            // resumed Store validate and publish its chain from bytes outside
            // the chain directory. It is now a hard stop under its own code,
            // whatever it points at.
            if !matches!(classified, None | Some(Cycle4StagedEntryV1::Foreign))
                && !is_regular_file_v1(&entry.path())
            {
                return Err(Cycle4ArmErrorV1::contract(
                    "cycle4_arm_v1_baseline_sidecar_irregular",
                    format!(
                        "{} carries a reserved sidecar name but is not a regular file",
                        entry.path().display()
                    ),
                ));
            }
            match classified {
                Some(Cycle4StagedEntryV1::Record(index)) => records.push(index),
                Some(Cycle4StagedEntryV1::Debris) => debris.push(entry.path()),
                Some(Cycle4StagedEntryV1::Foreign) if !exclusive => {}
                Some(Cycle4StagedEntryV1::Foreign) | None => {
                    return Err(Cycle4ArmErrorV1::contract(
                        "cycle4_arm_v1_baseline_sidecar_staging",
                        format!("{name} is not a staged sidecar record name"),
                    ))
                }
            }
        }
        records.sort_unstable();
        Ok((records, debris))
    }

    /// Every update index currently staged, ascending, across the current
    /// grammar in the chain directory and the legacy staging directory a
    /// previous layout may have left behind.
    fn staged_update_indexes_v1(&self) -> Result<Vec<u64>> {
        let (mut indexes, _) = Self::scan_staging_entries_v1(&self.chain_dir, false)?;
        let (legacy, _) = Self::scan_staging_entries_v1(&self.legacy_staged_dir_v1(), true)?;
        indexes.extend(legacy);
        indexes.sort_unstable();
        indexes.dedup();
        Ok(indexes)
    }

    /// Deletes exactly the two recognized debris grammars from both staging
    /// namespaces (review finding P2), so a Store left mid-publication by an
    /// earlier layout does not abort the first reconciliation under this one.
    fn sweep_staging_debris_v1(&self) -> Result<()> {
        let mut debris = Vec::new();
        for (dir, exclusive) in [
            (self.chain_dir.clone(), false),
            (self.legacy_staged_dir_v1(), true),
        ] {
            let (_, found) = Self::scan_staging_entries_v1(&dir, exclusive)?;
            debris.extend(found);
        }
        for path in debris {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(Cycle4ArmErrorV1::runtime(
                        "cycle4_arm_v1_baseline_sidecar_staging",
                        format!("removing debris {}: {error}", path.display()),
                    ))
                }
            }
        }
        Ok(())
    }

    /// Reconciles the staged area against the Store's committed tip, BEFORE
    /// the baseline-aware walk runs (review finding P1).
    ///
    /// A staged record whose update index is at or below `tip_update` belongs
    /// to an update the Store durably committed: it is left in place, served
    /// to the walk by `sidecar_record_bytes_v4`, revalidated there against
    /// that update's own committed evidence, and promoted only once the walk
    /// has accepted the whole Store. A staged record above the tip belongs to
    /// an update the Store never committed (the producer stages before
    /// publishing, so this is exactly the crash window between the two) and
    /// is discarded. A committed update with no sidecar at all is
    /// reconstructible only at the tip, which `may_reconstruct_sidecar_v4`
    /// authorizes from here.
    fn reconcile_staged_sidecars_v1(&self, tip_update: u64) -> Result<()> {
        // Debris first (review finding P2): an interrupted publication under
        // this layout or the previous one leaves a recognizable temporary,
        // and a scan that tripped over it would abort the whole reconcile.
        self.sweep_staging_debris_v1()?;
        for update_index in self.staged_update_indexes_v1()? {
            // Resolved for EVERY staged update, committed or not: this is
            // what turns a disagreement between the two staging layouts into
            // a named failure before anything is promoted.
            let Some(record) = self.resolve_staged_record_v1(update_index)? else {
                continue;
            };
            if update_index <= tip_update {
                continue;
            }
            for path in record.copies {
                std::fs::remove_file(&path).map_err(|error| {
                    Cycle4ArmErrorV1::runtime(
                        "cycle4_arm_v1_baseline_sidecar_staging",
                        format!("discarding {}: {error}", path.display()),
                    )
                })?;
            }
        }
        self.reconstructable_tip_update.set(Some(tip_update));
        Ok(())
    }

    fn sidecar_path_v1(&self, update_index: u64) -> Option<PathBuf> {
        sidecar_record_name_v4(update_index)
            .ok()
            .map(|name| self.chain_dir.join(name))
    }

    fn chain_record_bytes_v1(&self, generation_index: u64) -> Option<Vec<u8>> {
        let name = record_final_name_v4(generation_index).ok()?;
        std::fs::read(self.chain_dir.join(name)).ok()
    }

    fn chain_manifest_bytes_v1(&self, generation_index: u64) -> Option<Vec<u8>> {
        let name = manifest_final_name_v4(generation_index).ok()?;
        std::fs::read(self.chain_dir.join(name)).ok()
    }

    /// Registers one validated Store checkpoint's core train-state hash. The
    /// Store walk calls this for every boundary it proves, so a chain
    /// manifest is only ever decoded against a hash the Store itself
    /// authenticated.
    fn observe_v1(&self, generation_index: u64, core_state_sha256: [u8; 32]) {
        self.observed_core_hashes
            .borrow_mut()
            .insert(generation_index, core_state_sha256);
    }

    fn resolve_boundary_state_v1(&self, generation_index: u64) -> Option<NativeBaselineStateV4> {
        if let Some(state) = self.boundary_states.borrow().get(&generation_index) {
            return Some(state.clone());
        }
        let state = self.compute_boundary_state_v1(generation_index)?;
        self.boundary_states
            .borrow_mut()
            .insert(generation_index, state.clone());
        Some(state)
    }

    fn compute_boundary_state_v1(&self, generation_index: u64) -> Option<NativeBaselineStateV4> {
        if generation_index == 0 && self.chain_record_bytes_v1(0).is_none() {
            // Pre-first-publish genesis: the committed baseline is empty by
            // definition, and the launcher publishes the genesis chain record
            // for it before the first window trains.
            return Some(NativeBaselineStateV4::empty_v4());
        }
        if self.chain_record_bytes_v1(generation_index).is_some() {
            let manifest_bytes = self.chain_manifest_bytes_v1(generation_index)?;
            let core = {
                let observed = self.observed_core_hashes.borrow();
                *observed.get(&generation_index)?
            };
            let manifest = decode_checkpoint_manifest_v4(&manifest_bytes, core).ok()?;
            if manifest.generation_index() != generation_index {
                return None;
            }
            return Some(manifest.baseline_state());
        }
        // At most one checkpoint boundary past the chain tip: replay that
        // boundary's own sidecars forward from the previous boundary.
        let interval = self.checkpoint_segment_updates;
        if interval == 0 || generation_index < interval {
            return None;
        }
        let previous = generation_index.checked_sub(interval)?;
        if self.chain_record_bytes_v1(previous).is_none() && previous != 0 {
            return None;
        }
        let mut state = self.resolve_boundary_state_v1(previous)?;
        for update_index in (previous + 1)..=generation_index {
            state = self.replay_sidecar_v1(&state, update_index)?;
        }
        Some(state)
    }

    /// Applies one published sidecar's own observations to `prior`, then
    /// requires the result to equal the sidecar's declared `c_{t+1}` bits for
    /// every cell it names. A sidecar that disagrees with its own successor
    /// claim never advances the replay.
    fn replay_sidecar_v1(
        &self,
        prior: &NativeBaselineStateV4,
        update_index: u64,
    ) -> Option<NativeBaselineStateV4> {
        let bytes = self.sidecar_record_bytes_v4(update_index)?;
        let record = decode_update_baseline_record_v4(&bytes).ok()?;
        if record.update_index() != update_index {
            return None;
        }
        for cell in record.cells() {
            if prior.c_for_cell_v4(cell.key()).to_bits() != cell.c_t_bits() {
                return None;
            }
        }
        let observations = record
            .cells()
            .iter()
            .map(|cell| BaselineObservationV4 {
                key: cell.key().clone(),
                residual_sum_f64: cell.residual_sum_f64(),
                decision_count: cell.decision_count(),
                episode_count: cell.episode_count(),
            })
            .collect::<Vec<_>>();
        let successor = prior.apply_update_v4(&observations).ok()?;
        for cell in record.cells() {
            if successor.c_for_cell_v4(cell.key()).to_bits() != cell.c_next_bits() {
                return None;
            }
        }
        Some(successor)
    }
}

impl BaselineSidecarSourceV4 for Cycle4BaselineChainAccessV1 {
    fn sidecar_record_bytes_v4(&self, update_index: u64) -> Option<Vec<u8>> {
        // The promoted record is authoritative; a staged one is this
        // update's record until it is promoted. Both are on disk, so a
        // process that dies mid-segment leaves the staged copy for the next
        // open to reconcile rather than losing it with the process.
        // No-follow at every step: a reserved name that is a symlink is not
        // this update's record, whatever it points at.
        let promoted = self.sidecar_path_v1(update_index)?;
        if is_regular_file_v1(&promoted) {
            if let Ok(bytes) = std::fs::read(&promoted) {
                return Some(bytes);
            }
        }
        // A disagreement between the two staging layouts is reported as
        // absent here; `reconcile_staged_sidecars_v1` surfaces it under its
        // own code before any walk reaches this point.
        self.resolve_staged_record_v1(update_index)
            .ok()
            .flatten()
            .map(|record| record.bytes)
    }
}

impl BaselineChainAccessV4 for Cycle4BaselineChainAccessV1 {
    fn committed_state_for_generation_v4(
        &self,
        generation_index: u64,
    ) -> Option<NativeBaselineStateV4> {
        self.resolve_boundary_state_v1(generation_index)
    }

    fn observe_store_checkpoint_v4(&self, generation_index: u64, core_state_sha256: [u8; 32]) {
        self.observe_v1(generation_index, core_state_sha256);
    }

    fn may_reconstruct_sidecar_v4(&self, update_index: u64) -> bool {
        // Only the Store's own tip update, and only after a reconcile said
        // so. Every earlier committed update whose sidecar is gone is
        // unrecoverable evidence and must fail the walk closed.
        update_index != 0 && self.reconstructable_tip_update.get() == Some(update_index)
    }

    fn stage_sidecar_record_v4(&self, update_index: u64, record_bytes: &[u8]) -> bool {
        let Some(promoted) = self.sidecar_path_v1(update_index) else {
            return false;
        };
        // Crash replay: an already-promoted sidecar with identical bytes is
        // the same record, so accept it and stage nothing; different bytes
        // are a genuine conflict and fail closed. No-follow: a reserved name
        // that is not a regular file is never this update's record and is
        // never overwritten.
        if promoted.symlink_metadata().is_ok() {
            return is_regular_file_v1(&promoted)
                && std::fs::read(&promoted).is_ok_and(|existing| existing == record_bytes);
        }
        match self.resolve_staged_record_v1(update_index) {
            // Already staged: identical bytes are the same record, a
            // disagreement between the two layouts fails closed.
            Ok(Some(record)) => return record.bytes == record_bytes,
            Ok(None) => {}
            Err(_) => return false,
        }
        // Durable BEFORE the Store publishes the update this record explains
        // (review finding P1): the Store must never hold v4 evidence whose
        // sidecar exists only in this process's memory, nor whose name a
        // reboot can lose out from under a committed Store. Published INTO
        // the chain directory under the reserved staged suffix, so it is
        // exactly as durable as the chain records beside it and no new
        // directory entry has to survive with it.
        let Ok(final_name) = sidecar_record_name_v4(update_index) else {
            return false;
        };
        Self::publish_sidecar_bytes_v1(
            &self.chain_dir,
            &cycle4_staged_sidecar_name_v1(&final_name),
            record_bytes,
        )
        .is_ok()
    }

    fn commit_staged_sidecar_records_v4(&self) -> bool {
        let Ok(staged) = self.staged_update_indexes_v1() else {
            return false;
        };
        if staged.is_empty() {
            return true;
        }
        if std::fs::create_dir_all(&self.chain_dir).is_err() {
            return false;
        }
        // Ascending update order, so a crash mid-promotion leaves a prefix
        // of this segment promoted and the rest staged -- a shape the next
        // open's reconcile resolves exactly as it resolves any other.
        for update_index in staged {
            let (Some(promoted), Ok(final_name)) = (
                self.sidecar_path_v1(update_index),
                sidecar_record_name_v4(update_index),
            ) else {
                return false;
            };
            // Both layouts, compared: a conflict fails the commit closed
            // rather than promoting one copy and leaving the other behind.
            let Ok(Some(record)) = self.resolve_staged_record_v1(update_index) else {
                return false;
            };
            let promoted_existing = if is_regular_file_v1(&promoted) {
                std::fs::read(&promoted).ok()
            } else if promoted.symlink_metadata().is_ok() {
                // A reserved promoted name that is not a regular file is
                // never overwritten and never trusted.
                return false;
            } else {
                None
            };
            match promoted_existing {
                // Already promoted with identical bytes: drop the staged
                // copies and carry on, so a replayed promotion converges.
                Some(existing) if existing == record.bytes => {}
                Some(_) => return false,
                None => {
                    // Republished through the same durable primitive rather
                    // than renamed across directories: the promoted name must
                    // be as reboot-durable as the staged one it supersedes.
                    if Self::publish_sidecar_bytes_v1(&self.chain_dir, &final_name, &record.bytes)
                        .is_err()
                    {
                        return false;
                    }
                }
            }
            // EVERY copy drains, so a legacy leftover cannot outlive the
            // promotion that consumed it.
            for path in record.copies {
                if std::fs::remove_file(&path).is_err() {
                    return false;
                }
            }
        }
        // A drained legacy staging directory is removed; `remove_dir` refuses
        // a non-empty one, so this can never discard an unpromoted record.
        let _ = std::fs::remove_dir(self.legacy_staged_dir_v1());
        true
    }
}

/// Atomic publication: a create-new temporary in the destination's own
/// directory, written, flushed, and synced, then renamed into place. The
/// temporary is removed on any failure, so a failed publish never leaves a
/// partial record behind.
fn write_file_atomically_v1(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().map_or_else(
        || "cycle4-arm-record".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let temp_path = parent.join(format!("{file_name}.tmp-{}", std::process::id()));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Origin record
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle4ArmOriginRecordV1 {
    schema: String,
    arm_kind: String,
    run_sha256: String,
    base_seed: u64,
    init_generation: u64,
    parent_source_run_sha256: String,
    parent_checkpoint_sha256: String,
    parent_sidecar_sha256: String,
    parent_state_sha256: String,
    derived_model_parameter_sha256: String,
    /// The arm's own genesis checkpoint identity, exactly as the Store
    /// published it. It exists nowhere else until the Store does, and it is
    /// what the genesis refresh manifest's own-run slot must bind -- which is
    /// why the bootstrap publishes it here rather than binding a manifest
    /// that cannot exist yet.
    genesis_checkpoint_manifest_sha256: String,
    genesis_checkpoint_payload_sha256: String,
    genesis_model_parameter_sha256: String,
    genesis_train_state_sha256: String,
}

/// The four hashes that identify one published checkpoint, in the shape the
/// cycle-4 slot roster wants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cycle4ArmGenesisIdentityV1 {
    pub checkpoint_manifest_sha256: String,
    pub checkpoint_payload_sha256: String,
    pub model_parameter_sha256: String,
    pub train_state_sha256: String,
}

// ---------------------------------------------------------------------
// Contract validation
// ---------------------------------------------------------------------

#[derive(Debug)]
struct Cycle4ArmContractV1 {
    refresh_index: u64,
    /// The Store generation this manifest opens: `refresh_index * 128`.
    program_update: u64,
    /// `896 + program_update`, the same number in the contract's
    /// trainee-local numbering. Kept so the mapping is proven, not assumed.
    #[allow(dead_code)]
    trainee_local_generation: u64,
}

/// The record-level cycle-4 arm check, exposed for the run-record BUILDER
/// (`native_cycle4_run_record_v1`) so a record is proven acceptable to this
/// launcher before it is ever written, rather than only when the first
/// invocation reads it back. Exactly [`validate_run_contract_v1`], no
/// separate restatement: what the builder proves is what the launcher
/// enforces, by construction.
///
/// # Errors
///
/// Returns the same classified [`Cycle4ArmErrorV1`] an invocation would.
pub fn validate_cycle4_arm_run_record_v1(
    run: &ValidatedTrainRunV2,
    arm: Cycle4ArmKindV1,
) -> Result<()> {
    validate_run_contract_v1(run, arm)?;
    validate_device_contract_v1(run, arm)?;
    Ok(())
}

fn validate_run_contract_v1(run: &ValidatedTrainRunV2, arm: Cycle4ArmKindV1) -> Result<()> {
    let contracts = run.record().contracts();
    let program = contracts
        .population_program_v2_cycle4
        .as_ref()
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_missing_population_program_v2_cycle4",
                "the run record declares no population_program_v2_cycle4 section",
            )
        })?;
    if program.arm_kind != arm.wire_v1() {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_arm_kind_mismatch",
            format!(
                "requested {} but the run declares {}",
                arm.wire_v1(),
                program.arm_kind
            ),
        ));
    }
    if program.static_pool != arm.static_pool_v1() {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_arm_kind_mismatch",
            "static_pool disagrees with the arm kind",
        ));
    }
    if program.refresh_interval != CYCLE4_REFRESH_INTERVAL_V1 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_refresh_interval",
            program.refresh_interval.to_string(),
        ));
    }
    // Arm-kind consistency, restated here rather than trusted from decode:
    // treatment-rb and static-rb require the v4 trainer identity, control-r
    // forbids it.
    let declares_v4 = contracts.trainer_v4_candidate.is_some();
    if declares_v4 != arm.uses_baseline_v4_v1() {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_trainer_section_mismatch",
            format!(
                "{} {} trainer_v4_candidate",
                arm.wire_v1(),
                if arm.uses_baseline_v4_v1() {
                    "requires"
                } else {
                    "forbids"
                }
            ),
        ));
    }
    let loss_identity_is_v4 = matches!(
        contracts.trainer_loss_identity_v2(),
        TrainerLossIdentityV2::V4Candidate
    );
    if loss_identity_is_v4 != arm.uses_baseline_v4_v1() {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_trainer_section_mismatch",
            "declared loss identity disagrees with the arm kind",
        ));
    }
    Ok(())
}

/// Device contract: the numerical backend the Store binds must be the one
/// the arm's trainer admits. v4 admits only `CudaBurnDense`
/// (`docs/native_trainer_terminal_reinforce_value_v4_candidate_v1.md`
/// Section 5); CONTROL-R keeps whatever backend its own frozen record binds.
fn validate_device_contract_v1(
    run: &ValidatedTrainRunV2,
    arm: Cycle4ArmKindV1,
) -> Result<NativeTrainingNumericalBackendV1> {
    let backend = run.store_numerical_backend_v2().ok_or_else(|| {
        Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_device_contract",
            "the run record binds no Store numerical backend",
        )
    })?;
    if arm.uses_baseline_v4_v1() && backend != NativeTrainingNumericalBackendV1::CudaBurnDense {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_device_contract",
            "v4 arms admit only the CudaBurnDense numerical backend",
        ));
    }
    Ok(backend)
}

fn execution_config_from_run_v1(
    run: &ValidatedTrainRunV2,
    backend: NativeTrainingNumericalBackendV1,
) -> Result<NativeTrainingExecutionConfigV1> {
    let invalid = |detail: &str| {
        Cycle4ArmErrorV1::contract("cycle4_arm_v1_execution_config", detail.to_owned())
    };
    let record = run.record();
    let topology = record.topology();
    let value_coefficient_bits =
        u32::from_str_radix(&record.optimization().value_coefficient_f32_bits, 16)
            .map_err(|_| invalid("value_coefficient_f32_bits"))?;
    let learning_rate_bits = u32::from_str_radix(&record.optimization().learning_rate_f32_bits, 16)
        .map_err(|_| invalid("learning_rate_f32_bits"))?;
    Ok(NativeTrainingExecutionConfigV1 {
        run_base_seed: record.schedule().base_seed,
        batch_episodes: run.batch_episodes(),
        deck_ids: record.environment().deck_ids.clone(),
        max_physical_decisions: record.limits().max_physical_decisions,
        max_policy_steps: record.limits().max_policy_steps,
        worker_count: usize::try_from(topology.worker_count)
            .map_err(|_| invalid("worker_count"))?,
        sessions_per_worker: usize::try_from(topology.sessions_per_worker)
            .map_err(|_| invalid("sessions_per_worker"))?,
        broker_batch_target: usize::try_from(topology.broker_batch_target)
            .map_err(|_| invalid("broker_batch_target"))?,
        scheduler_timeout: Duration::from_millis(topology.scheduler_timeout_ms),
        measure_broker_service_time: topology.measure_broker_service_time,
        value_coefficient_bits,
        learning_rate_bits,
        numerical_backend: backend,
        backward_worker_limit: 1,
    })
}

// ---------------------------------------------------------------------
// Refresh manifest decode (content-resolving, with the predecessor chain)
// ---------------------------------------------------------------------

/// Decodes this interval's manifest with its panel bytes. Non-genesis
/// manifests bind their predecessor by hash, and the cycle-4 decoder has no
/// format-only acceptance path, so the whole chain is re-derived from the
/// manifest file's own directory using the pinned
/// `refresh-NN.manifest.json` / `refresh-NN.panel.json` naming scheme (the
/// same scheme `cycle4_refresh_build_v1` writes).
fn decode_interval_manifest_v1(
    manifest_path: &Path,
    panel_path: Option<&Path>,
) -> Result<Cycle4RefreshManifestV1> {
    let chain_dir = manifest_path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let manifest_bytes = std::fs::read(manifest_path).map_err(|error| {
        Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_refresh_manifest_read",
            format!("{}: {error}", manifest_path.display()),
        )
    })?;
    let panel_bytes = panel_path
        .map(|path| {
            std::fs::read(path).map_err(|error| {
                Cycle4ArmErrorV1::runtime(
                    "cycle4_arm_v1_payoff_panel_read",
                    format!("{}: {error}", path.display()),
                )
            })
        })
        .transpose()?;

    // Genesis first: it is the only manifest that decodes without a
    // predecessor, and it must carry no panel.
    let genesis_path = chain_dir.join(cycle4_chain_manifest_filename_v1(0));
    let genesis_bytes = std::fs::read(&genesis_path).map_err(|error| {
        Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_refresh_chain_read",
            format!("{}: {error}", genesis_path.display()),
        )
    })?;
    let mut current =
        decode_cycle4_refresh_manifest_v1(&genesis_bytes, None, None).map_err(|error| {
            Cycle4ArmErrorV1::contract("cycle4_arm_v1_refresh_manifest_rejected", error.to_string())
        })?;
    if genesis_bytes == manifest_bytes {
        if panel_bytes.is_some() {
            return Err(Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_genesis_takes_no_panel",
                "the genesis refresh manifest binds no payoff panel",
            ));
        }
        return Ok(current);
    }
    let panel_bytes = panel_bytes.ok_or_else(|| {
        Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_missing_payoff_panel",
            "a non-genesis refresh manifest requires its panel bytes",
        )
    })?;
    for refresh_index in 1..=CYCLE4_REFRESH_MAX_INDEX_V1 {
        let link_path = chain_dir.join(cycle4_chain_manifest_filename_v1(refresh_index));
        let link_bytes = std::fs::read(&link_path).map_err(|error| {
            Cycle4ArmErrorV1::runtime(
                "cycle4_arm_v1_refresh_chain_read",
                format!("{}: {error}", link_path.display()),
            )
        })?;
        let is_target = link_bytes == manifest_bytes;
        let link_panel = if is_target {
            panel_bytes.clone()
        } else {
            let panel_path = chain_dir.join(cycle4_chain_panel_filename_v1(refresh_index));
            std::fs::read(&panel_path).map_err(|error| {
                Cycle4ArmErrorV1::runtime(
                    "cycle4_arm_v1_refresh_chain_read",
                    format!("{}: {error}", panel_path.display()),
                )
            })?
        };
        current = decode_cycle4_refresh_manifest_v1(
            &link_bytes,
            Some(&current),
            Some(link_panel.as_slice()),
        )
        .map_err(|error| {
            Cycle4ArmErrorV1::contract("cycle4_arm_v1_refresh_manifest_rejected", error.to_string())
        })?;
        if is_target {
            return Ok(current);
        }
    }
    Err(Cycle4ArmErrorV1::contract(
        "cycle4_arm_v1_refresh_manifest_not_in_chain",
        format!(
            "{} is not a link of the chain rooted at {}",
            manifest_path.display(),
            genesis_path.display()
        ),
    ))
}

fn validate_manifest_against_run_v1(
    manifest: &Cycle4RefreshManifestV1,
    run: &ValidatedTrainRunV2,
    arm: Cycle4ArmKindV1,
) -> Result<Cycle4ArmContractV1> {
    if manifest.trainee_run_sha256_v1() != run.run_sha256()
        || manifest.trainee_base_seed_v1() != run.record().schedule().base_seed
    {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_manifest_run_binding",
            "the manifest's trainee identity is not this run",
        ));
    }
    if arm.static_pool_v1() && manifest.refresh_index_v1() != 0 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_static_pool_manifest_advanced",
            format!(
                "static-rb never advances past the genesis manifest, got refresh index {}",
                manifest.refresh_index_v1()
            ),
        ));
    }
    let program_update = manifest
        .refresh_index_v1()
        .checked_mul(CYCLE4_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_manifest_generation",
                "program update overflow",
            )
        })?;
    let trainee_local_generation = CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1
        .checked_add(program_update)
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_manifest_generation",
                "trainee-local generation overflow",
            )
        })?;
    if manifest.trainee_local_generation_v1() != trainee_local_generation {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_manifest_generation",
            "the manifest's trainee-local generation is not 896 plus its program update",
        ));
    }
    Ok(Cycle4ArmContractV1 {
        refresh_index: manifest.refresh_index_v1(),
        program_update,
        trainee_local_generation,
    })
}

/// The interval stop is a STORE generation, and it names the whole interval:
/// a multiple of the pre-registered 128 at or below the program's own 2048,
/// whose start is `stop - 128`.
///
/// The resume position may be anywhere inside that interval. A wrapper that
/// was interrupted mid-interval restarts the same process against the SAME
/// stop it was given, so the Store it reopens sits at whatever checkpoint
/// boundary the interrupted attempt reached (resume 388 for the interval
/// 384..=512, say). Every admissible position is therefore
/// `interval_start ..= stop`, and every one of them must be a checkpoint
/// segment boundary, because that is the only granularity the Store ever
/// publishes. `resume == stop` is the completed case: a process that
/// committed the interval's final generation and died before returning
/// trains nothing here, and the caller breaks immediately, revalidates the
/// whole Store, and returns the outcome the lost process would have
/// returned. The manifest position rule still compares the manifest's own
/// `program_update` against `interval_start`, the interval that manifest
/// opened, never against wherever the Store happens to sit.
///
/// `preflight_updates` is the bounded relaxation the CONTROL preflight ladder
/// needs and nothing else may use, and it is deliberately NOT widened the
/// same way: `Some(n)` keeps the exact `stop == resume + n` for `n` in
/// `1 ..= CYCLE4_ARM_PREFLIGHT_MAX_UPDATES_V1`, still inside the program's
/// end, still a whole number of checkpoint segments, and still pinned to the
/// genesis manifest below the first refresh boundary. A preflight prefix is
/// throwaway, so it has no interrupted-attempt case to serve.
fn validate_interval_stop_v1(
    stop_generation: u64,
    resume_generation: u64,
    checkpoint_segment_updates: u64,
    contract: &Cycle4ArmContractV1,
    arm: Cycle4ArmKindV1,
    preflight_updates: Option<u64>,
) -> Result<()> {
    let interval_stop =
        |detail: String| Cycle4ArmErrorV1::contract("cycle4_arm_v1_interval_stop", detail);
    if let Some(updates) = preflight_updates {
        return validate_preflight_stop_v1(
            stop_generation,
            resume_generation,
            checkpoint_segment_updates,
            contract,
            updates,
        );
    }
    if stop_generation > CYCLE4_ARM_STORE_GENERATION_TOTAL_V1 {
        return Err(interval_stop(format!(
            "stop generation {stop_generation} is past the program end {CYCLE4_ARM_STORE_GENERATION_TOTAL_V1}"
        )));
    }
    if stop_generation == 0 || !stop_generation.is_multiple_of(CYCLE4_REFRESH_INTERVAL_V1) {
        return Err(interval_stop(format!(
            "stop generation {stop_generation} is not a whole refresh interval"
        )));
    }
    let interval_start = stop_generation
        .checked_sub(CYCLE4_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            interval_stop(format!(
                "stop generation {stop_generation} is not one refresh interval past any start"
            ))
        })?;
    if checkpoint_segment_updates == 0
        || !CYCLE4_REFRESH_INTERVAL_V1.is_multiple_of(checkpoint_segment_updates)
    {
        return Err(interval_stop(
            "the training window must be a whole number of checkpoint segments".to_owned(),
        ));
    }
    if resume_generation < interval_start || resume_generation > stop_generation {
        return Err(interval_stop(format!(
            "the store resumes at {resume_generation}, outside the interval {interval_start}..={stop_generation} this stop names"
        )));
    }
    if !resume_generation.is_multiple_of(checkpoint_segment_updates) {
        return Err(interval_stop(format!(
            "the store resumes at {resume_generation}, which is not a boundary of its {checkpoint_segment_updates}-update checkpoint segment"
        )));
    }
    // A refresh-chained arm's manifest names the interval it opens; a
    // static-pool arm reuses the genesis manifest at every interval and
    // therefore binds no resume position of its own. Compared against the
    // interval's START, so an interrupted attempt resuming mid-interval is
    // judged against the same manifest as a fresh one.
    if !arm.static_pool_v1() && contract.program_update != interval_start {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_resume_position_mismatch",
            format!(
                "manifest refresh index {} opens store generation {}, but this stop names the interval {interval_start}..={stop_generation}",
                contract.refresh_index, contract.program_update
            ),
        ));
    }
    Ok(())
}

/// The preflight ladder's own stop rule, exactly as round D pinned it: an
/// exact `stop == resume + n` window of `1 ..= 8` updates, a whole number of
/// checkpoint segments, against the genesis manifest only, never past the
/// first refresh boundary.
fn validate_preflight_stop_v1(
    stop_generation: u64,
    resume_generation: u64,
    checkpoint_segment_updates: u64,
    contract: &Cycle4ArmContractV1,
    updates: u64,
) -> Result<()> {
    if updates == 0 || updates > CYCLE4_ARM_PREFLIGHT_MAX_UPDATES_V1 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_preflight_updates_range",
            format!(
                "--preflight-updates must be 1..={CYCLE4_ARM_PREFLIGHT_MAX_UPDATES_V1}, got {updates}"
            ),
        ));
    }
    let expected = resume_generation.checked_add(updates).ok_or_else(|| {
        Cycle4ArmErrorV1::contract("cycle4_arm_v1_interval_stop", "stop generation overflow")
    })?;
    if stop_generation != expected {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_interval_stop",
            format!("expected stop generation {expected}, got {stop_generation}"),
        ));
    }
    if stop_generation > CYCLE4_ARM_STORE_GENERATION_TOTAL_V1 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_interval_stop",
            format!(
                "stop generation {stop_generation} is past the program end {CYCLE4_ARM_STORE_GENERATION_TOTAL_V1}"
            ),
        ));
    }
    if checkpoint_segment_updates == 0 || !updates.is_multiple_of(checkpoint_segment_updates) {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_interval_stop",
            "the training window must be a whole number of checkpoint segments",
        ));
    }
    // A preflight prefix runs one or more short windows inside the genesis
    // interval and never chains a manifest, so it is pinned to the genesis
    // manifest and bounded below the first refresh boundary rather than
    // matched to a manifest position it does not have.
    if contract.refresh_index != 0 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_preflight_manifest_advanced",
            format!(
                "a preflight runs only against the genesis manifest, got refresh index {}",
                contract.refresh_index
            ),
        ));
    }
    if stop_generation > CYCLE4_REFRESH_INTERVAL_V1 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_preflight_manifest_advanced",
            format!(
                "a preflight never leaves the genesis interval, got stop generation {stop_generation}"
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------

/// Runs exactly one cycle-4 refresh interval for one arm and returns.
///
/// # Errors
///
/// Returns a classified [`Cycle4ArmErrorV1`]: `Contract` for any contract,
/// manifest, locator, or chain rejection (bin exit code 3) and `Runtime` for
/// an I/O or training failure (bin exit code 1).
#[allow(clippy::too_many_lines)]
pub fn run_native_cycle4_arm_v1(request: &Cycle4ArmRequestV1) -> Result<Cycle4ArmOutcomeV1> {
    // 1. Run contract, arm-kind consistency, and the device contract.
    let run_bytes = std::fs::read(&request.run_record).map_err(|error| {
        Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_run_record_read",
            format!("{}: {error}", request.run_record.display()),
        )
    })?;
    let run = decode_train_run_v2(&run_bytes).map_err(|error| {
        Cycle4ArmErrorV1::contract("cycle4_arm_v1_run_record_rejected", error.to_string())
    })?;
    validate_run_contract_v1(&run, request.arm)?;
    let backend = validate_device_contract_v1(&run, request.arm)?;
    let execution_config = execution_config_from_run_v1(&run, backend)?;

    // 2. The interval's manifest, its panel bytes, the locator, and the
    //    eight-slot engine.
    let manifest =
        decode_interval_manifest_v1(&request.refresh_manifest, request.payoff_panel.as_deref())?;
    let contract = validate_manifest_against_run_v1(&manifest, &run, request.arm)?;
    let locator_bytes = std::fs::read(&request.slot_locator).map_err(|error| {
        Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_slot_locator_read",
            format!("{}: {error}", request.slot_locator.display()),
        )
    })?;
    let locator = decode_slot_locator_v1(&locator_bytes)?;
    let slot_store_roots = slot_store_roots_for_manifest_v1(&locator, &manifest)?;

    // 3. Open or bootstrap the Store, authoring genesis from the pinned
    //    parent checkpoint when the Store is new.
    let (parent_dir, root_basename) = store_root_parts_v1(&request.store_root)?;
    let mode = if request.preflight_updates.is_some() {
        Cycle4ArmStoreModeV1::Preflight
    } else {
        Cycle4ArmStoreModeV1::Formal
    };
    // Verify the prefix admits this mode before touching the Store, but do not
    // WRITE the marker yet. `formal` and `preflight` are terminal, so writing
    // one on a prefix that turns out to hold no genesis would strand it: the
    // operator's next `--bootstrap-genesis` would be refused by a marker this
    // run had no business leaving behind. An unseeded prefix has to come out
    // of a rejected interval exactly as it went in, still bootstrap-eligible.
    verify_store_mode_marker_v1(&parent_dir, request.arm, &run, mode)?;
    let bootstrapped =
        bootstrap_native_training_store_v2(&parent_dir, &root_basename).map_err(|error| {
            Cycle4ArmErrorV1::runtime("cycle4_arm_v1_bootstrap_failed", error.to_string())
        })?;
    // Genesis is its own mode now. An interval invocation never authors one:
    // the genesis refresh manifest this invocation was handed can only have
    // been built AFTER the Store's genesis existed, so an unseeded Store here
    // means the two are out of order.
    if !bootstrapped.latest_final_present() {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_genesis_not_bootstrapped",
            format!(
                "{} holds no genesis; run --bootstrap-genesis before the first interval",
                request.store_root.display()
            ),
        ));
    }
    // Genesis is proven, so the mode may now be fixed for the life of the
    // prefix.
    claim_store_mode_marker_v1(&parent_dir, request.arm, &run, mode)?;
    let root = bootstrapped.into_root();

    // 4. Baseline chain: v4 arms only. CONTROL-R never installs one.
    let access = request.arm.uses_baseline_v4_v1().then(|| {
        Cycle4BaselineChainAccessV1::new_v1(
            request.chain_dir.clone(),
            run.checkpoint_segment_updates(),
        )
    });

    // Verify-or-publish, on every open and for every arm kind (review
    // finding P2): the origin record must exist and must bind this run, arm,
    // parent checkpoint, and the Store's own genesis checkpoint, whether or
    // not this invocation is the one that authored genesis. Runs before the
    // chain is touched, so a chain directory paired with the wrong run or arm
    // fails closed before anything is published into it.
    let genesis_identity = genesis_identity_from_store_v1(
        &root,
        &run,
        access
            .as_ref()
            .map(|access| access as &dyn BaselineChainAccessV4),
    )?;
    ensure_origin_record_v1(&request.chain_dir, request.arm, &run, &genesis_identity)?;

    let mut chain_generation = None;
    if let Some(access) = access.as_ref() {
        chain_generation = Some(prepare_baseline_chain_v1(&root, &run, access)?);
    }

    let engine = Arc::new(resolve_population_opponent_cycle4_v1(
        &manifest,
        &slot_store_roots,
        run.run_sha256(),
        access
            .as_ref()
            .map(|access| access as &dyn BaselineChainAccessV4),
    )?);

    // 5. Train exactly one interval.
    let mut session: Option<NativeTrainingStoreContinuationSessionV2> = None;
    let mut resume_generation: Option<u64> = None;
    let latest_generation_index = loop {
        let resumed = match access.as_ref() {
            None => resume_native_training_store_with_session_v2(
                &root,
                &run,
                execution_config.clone(),
                session.take(),
            ),
            Some(access) => resume_native_training_store_with_session_baseline_v4_v2(
                &root,
                &run,
                execution_config.clone(),
                session.take(),
                access,
            ),
        }
        .map_err(|error| {
            Cycle4ArmErrorV1::runtime("cycle4_arm_v1_resume_failed", error.to_string())
        })?;
        match resumed {
            NativeTrainingStoreResumeV2::Complete {
                latest_generation_index,
            } => {
                if resume_generation.is_none() {
                    resume_generation = Some(latest_generation_index);
                    validate_interval_stop_v1(
                        request.stop_generation,
                        latest_generation_index,
                        run.checkpoint_segment_updates(),
                        &contract,
                        request.arm,
                        request.preflight_updates,
                    )?;
                }
                // Review finding P2: `Complete` means the run record's own
                // `requested_successful_updates` is exhausted, which is not
                // the same as this interval being trained. A run whose
                // schedule stops short of the requested stop would otherwise
                // return a silently undertrained arm.
                if latest_generation_index != request.stop_generation {
                    return Err(Cycle4ArmErrorV1::contract(
                        "cycle4_arm_v1_interval_incomplete",
                        format!(
                            "the run's schedule completed at store generation {latest_generation_index}, short of the requested stop {}",
                            request.stop_generation
                        ),
                    ));
                }
                break latest_generation_index;
            }
            NativeTrainingStoreResumeV2::Continue(mut continuation) => {
                let parent_generation = continuation.parent_checkpoint.generation_index();
                if resume_generation.is_none() {
                    resume_generation = Some(parent_generation);
                    validate_interval_stop_v1(
                        request.stop_generation,
                        parent_generation,
                        run.checkpoint_segment_updates(),
                        &contract,
                        request.arm,
                        request.preflight_updates,
                    )?;
                }
                // The per-interval stop: never a multi-interval process.
                if parent_generation >= request.stop_generation {
                    break parent_generation;
                }
                continuation
                    .executor
                    .set_population_opponent_v1(Some(Arc::clone(&engine)));
                let prepared = match access.as_ref() {
                    None => {
                        continuation.executor.set_baseline_state_v4(None);
                        prepare_segment_v2(
                            &mut continuation.executor,
                            &run,
                            &continuation.parent_boundary,
                            &continuation.parent_checkpoint,
                        )
                    }
                    Some(access) => {
                        let state = access
                            .committed_state_for_generation_v4(parent_generation)
                            .ok_or_else(|| {
                                Cycle4ArmErrorV1::contract(
                                    "cycle4_arm_v1_baseline_boundary_missing",
                                    format!(
                                        "no committed baseline state for generation {parent_generation}"
                                    ),
                                )
                            })?;
                        continuation.executor.set_baseline_state_v4(Some(state));
                        prepare_segment_baseline_v4_v2(
                            &mut continuation.executor,
                            &run,
                            &continuation.parent_boundary,
                            &continuation.parent_checkpoint,
                            access,
                        )
                    }
                }
                .map_err(|error| {
                    Cycle4ArmErrorV1::runtime("cycle4_arm_v1_prepare_failed", error.code().to_owned())
                })?;
                let (receipt, next_session) = match access.as_ref() {
                    None => publish_prepared_segment_with_session_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                        &continuation.tip_proof,
                        continuation.windows_since_full_walk,
                    ),
                    Some(access) => publish_prepared_segment_with_session_baseline_v4_v2(
                        &root,
                        &run,
                        &continuation.parent_boundary,
                        &continuation.parent_checkpoint,
                        &prepared,
                        &continuation.tip_proof,
                        continuation.windows_since_full_walk,
                        access,
                    ),
                }
                .map_err(|error| {
                    Cycle4ArmErrorV1::runtime(
                        "cycle4_arm_v1_publish_failed",
                        error.code().to_owned(),
                    )
                })?;
                // The Store has now durably committed this segment's
                // evidence, so the sidecars staged while preparing it may
                // reach their immutable names. Before this point nothing was
                // written, which is what keeps a failed segment from leaving
                // sidecars for updates the Store does not contain.
                if let Some(access) = access.as_ref() {
                    if !access.commit_staged_sidecar_records_v4() {
                        return Err(Cycle4ArmErrorV1::runtime(
                            "cycle4_arm_v1_baseline_sidecar_commit",
                            "the segment's staged baseline sidecars could not be committed",
                        ));
                    }
                }
                // The boundary's chain record is published here, after the
                // Store durably committed the generation and before the
                // in-memory candidate is installed. A crash in this window
                // leaves the chain exactly one boundary behind the Store,
                // which is the `StoreAheadByOneBoundary` verdict resume
                // repairs; the reverse (a chain ahead of the Store) is never
                // producible.
                if let Some(access) = access.as_ref() {
                    let checkpoint = prepared.checkpoint_manifest_v2();
                    access.observe_v1(
                        checkpoint.generation_index(),
                        checkpoint.train_state_sha256(),
                    );
                    let baseline = access
                        .committed_state_for_generation_v4(checkpoint.generation_index())
                        .ok_or_else(|| {
                            Cycle4ArmErrorV1::contract(
                                "cycle4_arm_v1_baseline_boundary_missing",
                                format!(
                                    "no baseline state for the just-published generation {}",
                                    checkpoint.generation_index()
                                ),
                            )
                        })?;
                    chain_generation =
                        Some(publish_chain_boundary_v1(access, checkpoint, baseline)?);
                }
                prepared.commit_v2(receipt).map_err(|error| {
                    Cycle4ArmErrorV1::runtime(
                        "cycle4_arm_v1_commit_failed",
                        error.code().to_owned(),
                    )
                })?;
                session = Some(next_session);
            }
        }
    };

    // 6. Full-store validation on exit.
    let state = match access.as_ref() {
        None => validate_native_training_store_v2(&root, &run),
        Some(access) => validate_native_training_store_baseline_v4_v2(&root, &run, access),
    }
    .map_err(|error| {
        Cycle4ArmErrorV1::runtime("cycle4_arm_v1_validate_failed", error.to_string())
    })?;
    if state.latest_generation_index() != latest_generation_index {
        return Err(Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_validate_failed",
            "final validation disagrees with the trained tip",
        ));
    }

    Ok(Cycle4ArmOutcomeV1 {
        arm: request.arm,
        resume_generation_index: resume_generation.unwrap_or(latest_generation_index),
        latest_generation_index,
        trainee_local_generation: CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1
            .saturating_add(latest_generation_index),
        refresh_index: contract.refresh_index,
        refresh_manifest_sha256: lower_hex_raw32_v1(manifest.manifest_sha256_v1()),
        baseline_chain_generation: chain_generation,
    })
}

/// The Store prefix's mode marker: which of the two mutually exclusive modes
/// (formal or preflight) first claimed this prefix, and for which arm and run.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle4ArmModeMarkerV1 {
    schema: String,
    mode: String,
    arm_kind: String,
    run_sha256: String,
}

/// Which mode claimed a Store prefix.
///
/// `Bootstrap` is the pristine state `--bootstrap-genesis` leaves behind:
/// genesis is published but NOTHING has trained, so the prefix may still
/// become either a formal or a preflight Store. The first invocation that
/// trains fixes the mode, and from then on the other mode is refused. That is
/// the guarantee that actually matters, because only training can have run
/// under the relaxed interval check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cycle4ArmStoreModeV1 {
    Bootstrap,
    Formal,
    Preflight,
}

impl Cycle4ArmStoreModeV1 {
    const fn wire_v1(self) -> &'static str {
        match self {
            Self::Bootstrap => "bootstrap",
            Self::Formal => "formal",
            Self::Preflight => "preflight",
        }
    }
}

/// What a Store prefix's marker says about admitting one particular mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cycle4ArmStoreModeStateV1 {
    /// No marker on disk; claiming would create one.
    Absent,
    /// A marker that already names exactly this mode, arm and run.
    AlreadyClaimed,
    /// A `bootstrap` marker for this arm and run, which this mode may promote.
    PromotableFromBootstrap,
}

/// Proves `parent_dir`'s marker admits `mode`, WITHOUT writing anything.
///
/// Separating the check from the write is what lets a caller fail closed on a
/// wrong-mode invocation before it touches a Store, and still leave the prefix
/// exactly as it found it when some later precondition rejects the run. The
/// arm and the run identity are fixed by whoever claimed the prefix first. The
/// mode admits exactly one transition, `bootstrap -> formal` or
/// `bootstrap -> preflight`; `formal` and `preflight` are terminal, so a
/// preflight is rejected on any prefix a formal run has trained and the
/// reverse holds too.
fn verify_store_mode_marker_v1(
    parent_dir: &Path,
    arm: Cycle4ArmKindV1,
    run: &ValidatedTrainRunV2,
    mode: Cycle4ArmStoreModeV1,
) -> Result<Cycle4ArmStoreModeStateV1> {
    let expected = Cycle4ArmModeMarkerV1 {
        schema: CYCLE4_ARM_MODE_MARKER_SCHEMA_V1.to_owned(),
        mode: mode.wire_v1().to_owned(),
        arm_kind: arm.wire_v1().to_owned(),
        run_sha256: run.run_sha256().to_owned(),
    };
    let bytes = to_canonical_json_bytes_v1(&expected, CanonicalJsonNullPolicyV1::Forbid).map_err(
        |error| Cycle4ArmErrorV1::runtime("cycle4_arm_v1_mode_marker", error.to_string()),
    )?;
    let path = parent_dir.join(CYCLE4_ARM_MODE_MARKER_FILENAME_V1);
    match std::fs::read(&path) {
        Ok(existing) => {
            if existing == bytes {
                return Ok(Cycle4ArmStoreModeStateV1::AlreadyClaimed);
            }
            let actual: Cycle4ArmModeMarkerV1 =
                serde_json::from_slice(&existing).map_err(|error| {
                    Cycle4ArmErrorV1::contract(
                        "cycle4_arm_v1_mode_marker_conflict",
                        format!("{} is unreadable: {error}", path.display()),
                    )
                })?;
            let conflict = || {
                Cycle4ArmErrorV1::contract(
                    "cycle4_arm_v1_mode_marker_conflict",
                    format!(
                        "store prefix {} is already claimed by mode={} arm={} run={}, but this run is mode={} arm={} run={}",
                        parent_dir.display(),
                        actual.mode,
                        actual.arm_kind,
                        actual.run_sha256,
                        expected.mode,
                        expected.arm_kind,
                        expected.run_sha256,
                    ),
                )
            };
            if actual.arm_kind != expected.arm_kind || actual.run_sha256 != expected.run_sha256 {
                return Err(conflict());
            }
            // The one admissible transition: a prefix that was only
            // bootstrapped has trained nothing, so it may still become either
            // a formal or a preflight Store.
            if actual.mode == Cycle4ArmStoreModeV1::Bootstrap.wire_v1()
                && matches!(
                    mode,
                    Cycle4ArmStoreModeV1::Formal | Cycle4ArmStoreModeV1::Preflight
                )
            {
                return Ok(Cycle4ArmStoreModeStateV1::PromotableFromBootstrap);
            }
            Err(conflict())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(Cycle4ArmStoreModeStateV1::Absent)
        }
        Err(error) => Err(Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_mode_marker",
            format!("{}: {error}", path.display()),
        )),
    }
}

/// Claims `parent_dir` (the Store prefix) for this run's mode, or fails
/// closed. Verifies first and writes only when the marker is absent or is a
/// promotable `bootstrap`, so re-claiming an already-claimed prefix touches
/// nothing.
fn claim_store_mode_marker_v1(
    parent_dir: &Path,
    arm: Cycle4ArmKindV1,
    run: &ValidatedTrainRunV2,
    mode: Cycle4ArmStoreModeV1,
) -> Result<()> {
    if verify_store_mode_marker_v1(parent_dir, arm, run, mode)?
        == Cycle4ArmStoreModeStateV1::AlreadyClaimed
    {
        return Ok(());
    }
    let expected = Cycle4ArmModeMarkerV1 {
        schema: CYCLE4_ARM_MODE_MARKER_SCHEMA_V1.to_owned(),
        mode: mode.wire_v1().to_owned(),
        arm_kind: arm.wire_v1().to_owned(),
        run_sha256: run.run_sha256().to_owned(),
    };
    let bytes = to_canonical_json_bytes_v1(&expected, CanonicalJsonNullPolicyV1::Forbid).map_err(
        |error| Cycle4ArmErrorV1::runtime("cycle4_arm_v1_mode_marker", error.to_string()),
    )?;
    std::fs::create_dir_all(parent_dir).map_err(|error| {
        Cycle4ArmErrorV1::runtime("cycle4_arm_v1_mode_marker", error.to_string())
    })?;
    write_file_atomically_v1(&parent_dir.join(CYCLE4_ARM_MODE_MARKER_FILENAME_V1), &bytes)
        .map_err(|error| Cycle4ArmErrorV1::runtime("cycle4_arm_v1_mode_marker", error.to_string()))
}

// ---------------------------------------------------------------------
// Genesis bootstrap
// ---------------------------------------------------------------------

/// One genesis bootstrap's complete, typed request. Deliberately NOT a
/// variant of [`Cycle4ArmRequestV1`]: a bootstrap takes no refresh manifest,
/// no payoff panel, no stop generation, and no preflight window, because
/// nothing about a training interval applies to it.
#[derive(Clone, Debug)]
pub struct Cycle4ArmBootstrapRequestV1 {
    pub arm: Cycle4ArmKindV1,
    pub store_root: PathBuf,
    pub run_record: PathBuf,
    pub chain_dir: PathBuf,
    /// Only the locator's `genesis_parent_store_root` is used here; the eight
    /// slot entries are still decoded and structurally validated, because a
    /// malformed locator would fail the very first interval anyway and it
    /// costs nothing to say so before genesis is published.
    pub slot_locator: PathBuf,
}

/// What one genesis bootstrap actually published.
#[derive(Clone, Debug)]
pub struct Cycle4ArmBootstrapOutcomeV1 {
    pub arm: Cycle4ArmKindV1,
    pub run_sha256: String,
    pub base_seed: u64,
    /// Always 0: the Store's own genesis generation.
    pub genesis_generation_index: u64,
    /// Always 896: the same generation in the contract's trainee-local
    /// numbering, which is what the genesis manifest's own-run slot declares.
    pub trainee_local_generation: u64,
    pub genesis: Cycle4ArmGenesisIdentityV1,
}

/// Seeds one arm's Store from the pinned parent checkpoint and exits without
/// training, breaking the genesis circularity: the genesis refresh manifest's
/// own-run slot binds the arm's own generation-0 checkpoint, which cannot
/// exist until the Store does, and the Store cannot be opened by an interval
/// invocation without a manifest. This mode publishes the Store, the origin
/// record carrying that checkpoint's identity, and nothing else; the refresh
/// builder then authors `refresh-00.manifest.json` from it.
///
/// Idempotent on a Store that already holds ONLY genesis (review finding):
/// the Store commit and the origin-record publication are two writes, and a
/// process that died between them left a generation-0 Store with no origin
/// record. Rejecting that outright stranded the root, because the wrapper
/// then skips bootstrap (genesis exists) while every bootstrap retry is
/// refused. Such a Store is instead validated, its genesis identity read back
/// from its own checkpoint, the missing origin record published, and the same
/// outcome returned. A Store that has trained past generation 0 is still
/// refused: bootstrap never adopts a run in progress.
///
/// # Errors
///
/// Returns a classified [`Cycle4ArmErrorV1`]: `Contract` (bin exit code 3)
/// for any contract, locator, or already-trained-Store rejection, `Runtime`
/// (bin exit code 1) for an I/O or publication failure.
pub fn run_native_cycle4_arm_bootstrap_genesis_v1(
    request: &Cycle4ArmBootstrapRequestV1,
) -> Result<Cycle4ArmBootstrapOutcomeV1> {
    // 1. Exactly the run-contract, arm-kind, and device-contract validation a
    //    normal invocation performs, so a run record that could never train
    //    is rejected here rather than after a Store exists.
    let run_bytes = std::fs::read(&request.run_record).map_err(|error| {
        Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_run_record_read",
            format!("{}: {error}", request.run_record.display()),
        )
    })?;
    let run = decode_train_run_v2(&run_bytes).map_err(|error| {
        Cycle4ArmErrorV1::contract("cycle4_arm_v1_run_record_rejected", error.to_string())
    })?;
    validate_run_contract_v1(&run, request.arm)?;
    validate_device_contract_v1(&run, request.arm)?;

    // 2. The locator, for its genesis parent store root.
    let locator_bytes = std::fs::read(&request.slot_locator).map_err(|error| {
        Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_slot_locator_read",
            format!("{}: {error}", request.slot_locator.display()),
        )
    })?;
    let locator = decode_slot_locator_v1(&locator_bytes)?;

    // 3. Claim the prefix as bootstrapped, then open it.
    let (parent_dir, root_basename) = store_root_parts_v1(&request.store_root)?;
    claim_store_mode_marker_v1(
        &parent_dir,
        request.arm,
        &run,
        Cycle4ArmStoreModeV1::Bootstrap,
    )?;
    let bootstrapped =
        bootstrap_native_training_store_v2(&parent_dir, &root_basename).map_err(|error| {
            Cycle4ArmErrorV1::runtime("cycle4_arm_v1_bootstrap_failed", error.to_string())
        })?;
    let already_seeded = bootstrapped.latest_final_present();
    let root = bootstrapped.into_root();

    // 4. Publish genesis, unless a previous bootstrap already did. The Store
    //    commit and the origin-record write are two steps, so a crash between
    //    them leaves a generation-0 Store whose origin record is missing;
    //    this invocation finishes that publication instead of refusing it.
    let access = request.arm.uses_baseline_v4_v1().then(|| {
        Cycle4BaselineChainAccessV1::new_v1(
            request.chain_dir.clone(),
            run.checkpoint_segment_updates(),
        )
    });
    let genesis = if already_seeded {
        // A Store that has trained is never adopted: only the exact
        // generation-0 shape a bootstrap itself leaves behind.
        let state = match access.as_ref() {
            None => validate_native_training_store_v2(&root, &run),
            Some(access) => validate_native_training_store_baseline_v4_v2(&root, &run, access),
        }
        .map_err(|error| {
            Cycle4ArmErrorV1::runtime("cycle4_arm_v1_validate_failed", error.to_string())
        })?;
        bootstrap_may_adopt_seeded_store_v1(state.latest_generation_index(), &request.store_root)?;
        let identity = genesis_identity_from_checkpoint_v1(state.latest_checkpoint());
        ensure_origin_record_v1(&request.chain_dir, request.arm, &run, &identity)?;
        identity
    } else {
        author_genesis_from_parent_v1(&root, &run, &locator, &request.chain_dir, request.arm)?
    };

    // 5. The same final-store validation an interval exit performs.
    let state = match access.as_ref() {
        None => validate_native_training_store_v2(&root, &run),
        Some(access) => validate_native_training_store_baseline_v4_v2(&root, &run, access),
    }
    .map_err(|error| {
        Cycle4ArmErrorV1::runtime("cycle4_arm_v1_validate_failed", error.to_string())
    })?;
    if state.latest_generation_index() != 0 {
        return Err(Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_validate_failed",
            "a bootstrap must leave the Store at generation 0",
        ));
    }

    Ok(Cycle4ArmBootstrapOutcomeV1 {
        arm: request.arm,
        run_sha256: run.run_sha256().to_owned(),
        base_seed: run.record().schedule().base_seed,
        genesis_generation_index: 0,
        trainee_local_generation: CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1,
        genesis,
    })
}

/// Whether a bootstrap retry may adopt a Store that already carries a
/// published generation.
///
/// Only the exact shape a bootstrap itself leaves behind is adoptable:
/// generation 0 and nothing more, which is what a process that committed the
/// Store and died before publishing the origin record leaves. Any trained
/// Store is refused, so `--bootstrap-genesis` can never be pointed at a run
/// in progress and can never re-enter a prefix an interval has advanced.
fn bootstrap_may_adopt_seeded_store_v1(
    latest_generation_index: u64,
    store_root: &Path,
) -> Result<()> {
    if latest_generation_index != 0 {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_genesis_already_present",
            format!(
                "{} has trained to generation {latest_generation_index}; --bootstrap-genesis only ever seeds a new Store or completes an interrupted one",
                store_root.display()
            ),
        ));
    }
    Ok(())
}

fn store_root_parts_v1(store_root: &Path) -> Result<(PathBuf, String)> {
    let parent = store_root
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_store_root",
                "--store-root must name a directory inside a parent directory",
            )
        })?;
    let basename = store_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_store_root",
                "--store-root must end in a valid directory name",
            )
        })?;
    Ok((parent.to_path_buf(), basename.to_owned()))
}

/// Seeds the arm's new Store from the exact parent checkpoint the run record
/// pins. The generation the Store publishes is 0 (the frozen
/// `GenesisInitializationV2` path); the parent's generation survives as the
/// hashed claim in `contracts.opponent_ladder_initialization` and in this
/// launcher's own origin record.
fn author_genesis_from_parent_v1(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    locator: &Cycle4SlotLocatorV1,
    chain_dir: &Path,
    arm: Cycle4ArmKindV1,
) -> Result<Cycle4ArmGenesisIdentityV1> {
    let declared = declared_origin_v1(run)?;
    let parent_root = locator.genesis_parent_store_root.as_ref().ok_or_else(|| {
        Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_genesis_parent_missing",
            "the slot locator carries no genesis_parent_store_root",
        )
    })?;
    let parent_root = PathBuf::from(parent_root);
    let staged = stage_ladder_checkpoint_initialization_v1(&parent_root, declared.generation)
        .map_err(|error| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_genesis_parent_rejected",
                format!("{}: {error}", parent_root.display()),
            )
        })?;
    if &staged != declared {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_genesis_origin_mismatch",
            "the parent store does not reproduce the run record's pinned origin",
        ));
    }
    let checkpoint_ref = ladder_init_as_checkpoint_ref_v1(declared);
    let authority =
        resolve_ladder_checkpoint_authority_v1(&parent_root, &checkpoint_ref).map_err(|error| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_genesis_parent_rejected",
                format!("{}: {error}", parent_root.display()),
            )
        })?;
    let (parent_checkpoint, parent_payload) = authority.into_checkpoint_and_payload();
    let genesis_error =
        |detail: String| Cycle4ArmErrorV1::runtime("cycle4_arm_v1_genesis_failed", detail);
    let payload = derive_genesis_weights_only_payload_v2_v3(&parent_payload)
        .map_err(|error| genesis_error(error.to_string()))?;
    let checkpoint = build_genesis_checkpoint_manifest_v2_v3(run, &parent_checkpoint, &payload)
        .map_err(|error| genesis_error(error.to_string()))?;
    let segment = build_genesis_segment_manifest_v2(run, &checkpoint)
        .map_err(|error| genesis_error(error.to_string()))?;
    let boundary = build_genesis_native_training_boundary_v2(run, &segment, &checkpoint)
        .map_err(|error| genesis_error(error.to_string()))?;
    let reference = build_checkpoint_reference_v2(run, &boundary)
        .map_err(|error| genesis_error(error.to_string()))?;
    let latest =
        build_latest_v2(&boundary, &reference).map_err(|error| genesis_error(error.to_string()))?;
    let receipt = publish_genesis_generation_v2(
        root,
        run,
        &payload,
        &checkpoint,
        &segment,
        &boundary,
        &reference,
        &latest,
    )
    .map_err(|error| genesis_error(error.code().to_owned()))?;
    if receipt.generation_index() != 0 {
        return Err(genesis_error(
            "genesis publication reported a nonzero generation".to_owned(),
        ));
    }
    // Taken from the manifest the Store just published, not re-read off disk:
    // these are the same bytes `publish_genesis_generation_v2` committed.
    let identity = genesis_identity_from_checkpoint_v1(&checkpoint);
    ensure_origin_record_v1(chain_dir, arm, run, &identity)?;
    Ok(identity)
}

/// The run record's pinned genesis origin, which every cycle-4 arm must
/// declare whether or not its Store still needs authoring. The origin record
/// restates it, so the record cannot be built or verified without it.
fn declared_origin_v1(
    run: &ValidatedTrainRunV2,
) -> Result<&crate::native_training_store_run_v2::OpponentLadderInitializationContractV1> {
    run.record()
        .contracts()
        .opponent_ladder_initialization
        .as_ref()
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_genesis_requires_origin",
                "a cycle-4 arm's genesis must be seeded from a pinned parent checkpoint",
            )
        })
}

/// The four genesis-checkpoint hashes the origin record carries, read off one
/// already-validated checkpoint manifest.
fn genesis_identity_from_checkpoint_v1(
    checkpoint: &CheckpointManifestV3,
) -> Cycle4ArmGenesisIdentityV1 {
    Cycle4ArmGenesisIdentityV1 {
        checkpoint_manifest_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256()),
        checkpoint_payload_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()),
        model_parameter_sha256: lower_hex_raw32_v1(checkpoint.model_parameter_sha256()),
        train_state_sha256: lower_hex_raw32_v1(checkpoint.train_state_sha256()),
    }
}

/// The same four hashes, taken from an already-seeded Store's own generation
/// 0 through the ordinary validated boundary walk. This is how an invocation
/// that did NOT author genesis reconstructs what the origin record must say.
/// A `trainer_v4_candidate` Store resolves through `access`, exactly as its
/// own roster slot does; generation 0 carries no update evidence either way.
fn genesis_identity_from_store_v1(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    access: Option<&dyn BaselineChainAccessV4>,
) -> Result<Cycle4ArmGenesisIdentityV1> {
    let boundary = match access {
        Some(access) => load_native_training_boundary_baseline_v4_v2(root, run, 0, access),
        None => load_native_training_boundary_v2(root, run, 0),
    }
    .map_err(|error| {
        Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_genesis_identity",
            format!("store generation 0: {error}"),
        )
    })?;
    Ok(genesis_identity_from_checkpoint_v1(boundary.checkpoint()))
}

/// Publishes the launcher's hashed origin record if the chain directory has
/// none, and otherwise decodes the existing one and requires it to equal the
/// record this run, arm, and genesis checkpoint imply (review finding P2).
///
/// Called on EVERY open, for every arm kind including CONTROL-R, not only
/// when the Store's genesis is authored: a process that published genesis and
/// then failed to write the origin record would otherwise see `latest.json`
/// on its retry, skip authoring, and leave the origin unrecorded forever.
/// Verifying an existing record on every open additionally means the record
/// is read, not merely written, so a chain directory paired with the wrong
/// run, arm, or Store fails closed here.
fn ensure_origin_record_v1(
    chain_dir: &Path,
    arm: Cycle4ArmKindV1,
    run: &ValidatedTrainRunV2,
    identity: &Cycle4ArmGenesisIdentityV1,
) -> Result<()> {
    let declared = declared_origin_v1(run)?;
    let expected = Cycle4ArmOriginRecordV1 {
        schema: CYCLE4_ARM_ORIGIN_RECORD_SCHEMA_V1.to_owned(),
        arm_kind: arm.wire_v1().to_owned(),
        run_sha256: run.run_sha256().to_owned(),
        base_seed: run.record().schedule().base_seed,
        init_generation: declared.generation,
        parent_source_run_sha256: declared.source_run_sha256.clone(),
        parent_checkpoint_sha256: declared.checkpoint_sha256.clone(),
        parent_sidecar_sha256: declared.sidecar_sha256.clone(),
        parent_state_sha256: declared.state_sha256.clone(),
        derived_model_parameter_sha256: declared.derived_model_parameter_sha256.clone(),
        genesis_checkpoint_manifest_sha256: identity.checkpoint_manifest_sha256.clone(),
        genesis_checkpoint_payload_sha256: identity.checkpoint_payload_sha256.clone(),
        genesis_model_parameter_sha256: identity.model_parameter_sha256.clone(),
        genesis_train_state_sha256: identity.train_state_sha256.clone(),
    };
    let bytes = to_canonical_json_bytes_v1(&expected, CanonicalJsonNullPolicyV1::Forbid).map_err(
        |error| Cycle4ArmErrorV1::runtime("cycle4_arm_v1_origin_record", error.to_string()),
    )?;
    let path = chain_dir.join(CYCLE4_ARM_ORIGIN_RECORD_FILENAME_V1);
    if let Ok(existing) = std::fs::read(&path) {
        let decoded: Cycle4ArmOriginRecordV1 =
            from_canonical_json_bytes_v1(&existing, CanonicalJsonNullPolicyV1::Forbid).map_err(
                |error| {
                    Cycle4ArmErrorV1::contract(
                        "cycle4_arm_v1_origin_record_conflict",
                        format!("the existing origin record does not decode: {error}"),
                    )
                },
            )?;
        if decoded != expected {
            return Err(Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_origin_record_conflict",
                "the existing origin record does not bind this run, arm, parent checkpoint, and genesis checkpoint",
            ));
        }
        return Ok(());
    }
    std::fs::create_dir_all(chain_dir).map_err(|error| {
        Cycle4ArmErrorV1::runtime("cycle4_arm_v1_origin_record", error.to_string())
    })?;
    write_file_atomically_v1(&path, &bytes).map_err(|error| {
        Cycle4ArmErrorV1::runtime("cycle4_arm_v1_origin_record", error.to_string())
    })
}

/// Resumes the baseline chain under the pairing rule before any window
/// trains, publishing the one boundary record a crash between the Store
/// commit and the chain publish can leave missing. Returns the chain's tip
/// generation.
fn prepare_baseline_chain_v1(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
    access: &Cycle4BaselineChainAccessV1,
) -> Result<u64> {
    // Review finding P1: reconcile the staged sidecar area against the
    // Store's committed tip BEFORE the walk. Staged records for committed
    // updates stay (the walk revalidates them against that update's own
    // evidence, and they are promoted below once it accepts the whole
    // Store); staged records for updates the Store never committed are
    // discarded; a committed tip update with no sidecar at all is
    // reconstructed inside the walk from its own evidence. The tip is read
    // straight off `latest.json`, because the walk that would otherwise
    // report it is the very thing this unblocks.
    let tip_update = peek_latest_generation_index_from_store_v2(root).map_err(|error| {
        Cycle4ArmErrorV1::runtime("cycle4_arm_v1_validate_failed", error.to_string())
    })?;
    access.reconcile_staged_sidecars_v1(tip_update)?;
    // The full v4 walk both proves every persisted update's evidence against
    // the chain and hands back the Store's own core train-state hash per
    // boundary, which is the only admissible input to a chain manifest
    // decode.
    let state =
        validate_native_training_store_baseline_v4_v2(root, run, access).map_err(|error| {
            Cycle4ArmErrorV1::runtime("cycle4_arm_v1_validate_failed", error.to_string())
        })?;
    // Every staged record the walk just accepted is now durable evidence of
    // a committed update, so it takes its immutable name.
    if !access.commit_staged_sidecar_records_v4() {
        return Err(Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_baseline_sidecar_commit",
            "the reconciled baseline sidecars could not be promoted",
        ));
    }
    let store_checkpoints = state.checkpoint_core_state_sha256_v4().to_vec();
    if store_checkpoints.is_empty() {
        return Err(Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_validate_failed",
            "the full walk produced no checkpoint core hashes",
        ));
    }
    let resume = resume_baseline_chain_v4(
        &access.chain_dir,
        run.run_sha256(),
        store_checkpoints.as_slice(),
    )
    .map_err(|error| {
        Cycle4ArmErrorV1::contract("cycle4_arm_v1_baseline_chain_rejected", error.to_string())
    })?;
    match resume.verdict() {
        BaselineChainResumeVerdictV4::Clean => {}
        BaselineChainResumeVerdictV4::StoreAheadByOneBoundary => {
            let (generation, _core) = *store_checkpoints
                .last()
                .expect("nonempty store checkpoints");
            let baseline = access
                .committed_state_for_generation_v4(generation)
                .ok_or_else(|| {
                    Cycle4ArmErrorV1::contract(
                        "cycle4_arm_v1_baseline_boundary_missing",
                        format!("cannot reconstruct the baseline at generation {generation}"),
                    )
                })?;
            let checkpoint = state.latest_checkpoint();
            if checkpoint.generation_index() != generation {
                return Err(Cycle4ArmErrorV1::runtime(
                    "cycle4_arm_v1_baseline_chain_rejected",
                    "the walked tip disagrees with the checkpoint core hash list",
                ));
            }
            return publish_chain_boundary_v1(access, checkpoint, baseline);
        }
    }
    resume.generation_index().ok_or_else(|| {
        Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_baseline_chain_rejected",
            "a clean chain must have a tip",
        )
    })
}

/// Publishes one checkpoint-boundary chain record, binding the Store's own
/// committed checkpoint facts to the committed baseline state. Returns the
/// boundary generation.
fn publish_chain_boundary_v1(
    access: &Cycle4BaselineChainAccessV1,
    checkpoint: &CheckpointManifestV3,
    baseline: NativeBaselineStateV4,
) -> Result<u64> {
    let generation = checkpoint.generation_index();
    access.observe_v1(generation, checkpoint.train_state_sha256());
    if access.chain_record_bytes_v1(generation).is_some() {
        return Ok(generation);
    }
    let previous = generation
        .checked_sub(access.checkpoint_segment_updates)
        .and_then(|previous| access.chain_record_bytes_v1(previous))
        .map(|bytes| sha256_v1(&bytes));
    publish_baseline_record_v4(
        &access.chain_dir,
        BaselineChainRecordPartsV4 {
            expected_previous_record_sha256: previous,
        },
        checkpoint_manifest_parts_v4_from_v3(checkpoint, baseline),
    )
    .map_err(|error| {
        Cycle4ArmErrorV1::runtime("cycle4_arm_v1_baseline_chain_publish", error.to_string())
    })?;
    access.boundary_states.borrow_mut().remove(&generation);
    Ok(generation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_policy_baseline_state_v4::{BaselineCellKeyV4, BaselineRoleV4};
    use crate::native_population_refresh_manifest_cycle4_v1::{
        build_cycle4_refresh_manifest_v1, FrozenOccupantIdentityCycle4V1, CYCLE4_ANCHOR_0_V1,
        CYCLE4_ANCHOR_1_V1, CYCLE4_CURRENT_0_V1, CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1,
        CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1, CYCLE4_EXPLOITER_0_V1, CYCLE4_EXPLOITER_1_V1,
        CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1, CYCLE4_HISTORICAL_1_ROTATION_V1,
        CYCLE4_HISTORICAL_LAG_V1,
    };
    use crate::native_training_store_run_v2::{
        test_fixture_bytes_population_program_v2_cycle4_seeded_v1,
        test_fixture_bytes_population_program_v2_cycle4_v1,
    };
    use crate::native_training_store_update_group_v4::{
        build_update_baseline_record_v4, UpdateBaselineCellPartsV4, UpdateBaselineRecordPartsV4,
    };

    /// The one mechanism the durable move primitive may report on this
    /// platform: `MOVEFILE_WRITE_THROUGH` on Windows, a no-replace rename
    /// plus a directory `sync_all` everywhere else.
    #[cfg(windows)]
    const EXPECTED_DURABLE_MECHANISM_V1: ImmutableMoveMechanismV2 =
        ImmutableMoveMechanismV2::WindowsMoveFileExWriteThroughNoReplace;
    #[cfg(not(windows))]
    const EXPECTED_DURABLE_MECHANISM_V1: ImmutableMoveMechanismV2 =
        ImmutableMoveMechanismV2::UnixAtomicRenameNoReplaceDirectorySynced;

    const ROLES_V1: [&str; CYCLE4_SLOT_COUNT_V1] = [
        "anchor-0",
        "anchor-1",
        "historical-0",
        "historical-1",
        "current-0",
        "current-1",
        "exploiter-0",
        "exploiter-1",
    ];

    /// A fresh, uniquely named directory under the OS temp root for one
    /// test's exclusive use (the convention `cycle4_refresh_build_v1.rs`'s
    /// own tests follow). The caller removes it when done.
    fn fresh_temp_dir_v1(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mtg-kernel-cycle4-arm-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn digest_v1(tag: u8, nonce: u8) -> String {
        let mut bytes = [0_u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = tag
                .wrapping_mul(17)
                .wrapping_add(nonce)
                .wrapping_add(index as u8)
                .wrapping_add(0x31);
        }
        lower_hex_raw32_v1(bytes)
    }

    fn frozen_slot_v1(
        index: usize,
        frozen: &FrozenOccupantIdentityCycle4V1,
        weight_units: u64,
    ) -> Cycle4RefreshSlotV1 {
        Cycle4RefreshSlotV1 {
            slot_index: index as u64,
            role: ROLES_V1[index].to_owned(),
            occupant_class: if index >= 6 {
                "historical-fallback".to_owned()
            } else {
                "policy".to_owned()
            },
            source_base_seed: frozen.source_base_seed,
            source_run_sha256: frozen.source_run_sha256.to_owned(),
            source_generation: frozen.source_generation,
            checkpoint_manifest_sha256: frozen.checkpoint_manifest_sha256.to_owned(),
            checkpoint_payload_sha256: frozen.checkpoint_payload_sha256.to_owned(),
            model_parameter_sha256: frozen.model_parameter_sha256.to_owned(),
            train_state_sha256: frozen.train_state_sha256.to_owned(),
            weight_units,
        }
    }

    fn derived_slot_v1(
        index: usize,
        source_base_seed: u64,
        source_run_sha256: &str,
        source_generation: u64,
        nonce: u8,
        weight_units: u64,
    ) -> Cycle4RefreshSlotV1 {
        Cycle4RefreshSlotV1 {
            slot_index: index as u64,
            role: ROLES_V1[index].to_owned(),
            occupant_class: "policy".to_owned(),
            source_base_seed,
            source_run_sha256: source_run_sha256.to_owned(),
            source_generation,
            checkpoint_manifest_sha256: digest_v1(1, nonce),
            checkpoint_payload_sha256: digest_v1(2, nonce),
            model_parameter_sha256: digest_v1(3, nonce),
            train_state_sha256: digest_v1(4, nonce),
            weight_units,
        }
    }

    /// A trainee identity that is deliberately NOT any fixture run's, used
    /// to prove the manifest/run binding gate.
    fn foreign_run_sha256_v1() -> String {
        digest_v1(9, 9)
    }

    const FOREIGN_BASE_SEED_V1: u64 = 977_004;

    fn manifest_slots_v1(
        refresh_index: u64,
        trainee_run_sha256: &str,
        trainee_base_seed: u64,
    ) -> Vec<Cycle4RefreshSlotV1> {
        let weight = CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1;
        let trainee_local_generation =
            CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 + refresh_index * CYCLE4_REFRESH_INTERVAL_V1;
        let historical_generation = trainee_local_generation - CYCLE4_HISTORICAL_LAG_V1;
        let (historical_seed, historical_run) = if refresh_index <= 3 {
            (
                CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1,
                CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1.to_owned(),
            )
        } else {
            (trainee_base_seed, trainee_run_sha256.to_owned())
        };
        let rotation = &CYCLE4_HISTORICAL_1_ROTATION_V1[(refresh_index % 3) as usize];
        vec![
            frozen_slot_v1(0, &CYCLE4_ANCHOR_0_V1, weight),
            frozen_slot_v1(1, &CYCLE4_ANCHOR_1_V1, weight),
            derived_slot_v1(
                2,
                historical_seed,
                &historical_run,
                historical_generation,
                21,
                weight,
            ),
            frozen_slot_v1(3, rotation, weight),
            frozen_slot_v1(4, &CYCLE4_CURRENT_0_V1, weight),
            derived_slot_v1(
                5,
                trainee_base_seed,
                trainee_run_sha256,
                trainee_local_generation,
                55,
                weight,
            ),
            frozen_slot_v1(6, &CYCLE4_EXPLOITER_0_V1, weight),
            frozen_slot_v1(7, &CYCLE4_EXPLOITER_1_V1, weight),
        ]
    }

    fn genesis_manifest_for_v1(
        trainee_run_sha256: &str,
        trainee_base_seed: u64,
    ) -> Cycle4RefreshManifestV1 {
        build_cycle4_refresh_manifest_v1(
            0,
            None,
            None,
            trainee_run_sha256,
            trainee_base_seed,
            manifest_slots_v1(0, trainee_run_sha256, trainee_base_seed),
        )
        .expect("genesis manifest must build")
    }

    fn genesis_manifest_v1() -> Cycle4RefreshManifestV1 {
        genesis_manifest_for_v1(&foreign_run_sha256_v1(), FOREIGN_BASE_SEED_V1)
    }

    fn refresh_one_manifest_for_v1(
        genesis: &Cycle4RefreshManifestV1,
        panel_bytes: &[u8],
        trainee_run_sha256: &str,
        trainee_base_seed: u64,
    ) -> Cycle4RefreshManifestV1 {
        build_cycle4_refresh_manifest_v1(
            1,
            Some(genesis),
            Some(panel_bytes),
            trainee_run_sha256,
            trainee_base_seed,
            manifest_slots_v1(1, trainee_run_sha256, trainee_base_seed),
        )
        .expect("refresh-1 manifest must build")
    }

    fn refresh_one_manifest_v1(
        genesis: &Cycle4RefreshManifestV1,
        panel_bytes: &[u8],
    ) -> Cycle4RefreshManifestV1 {
        refresh_one_manifest_for_v1(
            genesis,
            panel_bytes,
            &foreign_run_sha256_v1(),
            FOREIGN_BASE_SEED_V1,
        )
    }

    fn locator_for_v1(manifest: &Cycle4RefreshManifestV1) -> Cycle4SlotLocatorV1 {
        Cycle4SlotLocatorV1 {
            schema: CYCLE4_ARM_SLOT_LOCATOR_SCHEMA_V1.to_owned(),
            stores: manifest
                .slots_v1()
                .iter()
                .map(|slot| Cycle4SlotLocatorEntryV1 {
                    checkpoint_manifest_sha256: slot.checkpoint_manifest_sha256.clone(),
                    store_root: format!("D:/cycle4/slot-{}", slot.slot_index),
                })
                .collect(),
            genesis_parent_store_root: None,
        }
    }

    fn run_for_arm_v1(arm: Cycle4ArmKindV1) -> ValidatedTrainRunV2 {
        let bytes = test_fixture_bytes_population_program_v2_cycle4_v1(arm.wire_v1());
        decode_train_run_v2(&bytes).expect("cycle-4 fixture must validate")
    }

    /// The same arm record, carrying the `opponent_ladder_initialization`
    /// section a real arm declares. Every Store that reached an interval was
    /// bootstrapped from that section, so the origin record can always be
    /// rebuilt from it.
    fn seeded_run_for_arm_v1(arm: Cycle4ArmKindV1) -> ValidatedTrainRunV2 {
        let bytes = test_fixture_bytes_population_program_v2_cycle4_seeded_v1(arm.wire_v1());
        decode_train_run_v2(&bytes).expect("seeded cycle-4 fixture must validate")
    }

    fn genesis_identity_fixture_v1(nonce: u8) -> Cycle4ArmGenesisIdentityV1 {
        Cycle4ArmGenesisIdentityV1 {
            checkpoint_manifest_sha256: digest_v1(11, nonce),
            checkpoint_payload_sha256: digest_v1(12, nonce),
            model_parameter_sha256: digest_v1(13, nonce),
            train_state_sha256: digest_v1(14, nonce),
        }
    }

    // ------------------------------------------------------------------
    // Contract gates
    // ------------------------------------------------------------------

    #[test]
    fn each_arm_accepts_its_own_run_record_v1() {
        for arm in [
            Cycle4ArmKindV1::ControlR,
            Cycle4ArmKindV1::StaticRb,
            Cycle4ArmKindV1::TreatmentRb,
        ] {
            let run = run_for_arm_v1(arm);
            validate_run_contract_v1(&run, arm).expect("matching arm must validate");
        }
    }

    #[test]
    fn arm_flag_must_match_the_run_record_v1() {
        let run = run_for_arm_v1(Cycle4ArmKindV1::ControlR);
        for arm in [Cycle4ArmKindV1::StaticRb, Cycle4ArmKindV1::TreatmentRb] {
            let error = validate_run_contract_v1(&run, arm).expect_err("mismatch must fail closed");
            assert_eq!(error.failure_v1(), Cycle4ArmFailureV1::Contract);
            assert_eq!(error.code_v1(), "cycle4_arm_v1_arm_kind_mismatch");
        }
    }

    #[test]
    fn arm_kind_and_trainer_section_agree_v1() {
        // The pairing the contract pins: control-r forbids the v4 trainer
        // section, the two rb arms require it.
        assert!(!Cycle4ArmKindV1::ControlR.uses_baseline_v4_v1());
        assert!(Cycle4ArmKindV1::StaticRb.uses_baseline_v4_v1());
        assert!(Cycle4ArmKindV1::TreatmentRb.uses_baseline_v4_v1());
        for arm in [
            Cycle4ArmKindV1::ControlR,
            Cycle4ArmKindV1::StaticRb,
            Cycle4ArmKindV1::TreatmentRb,
        ] {
            let run = run_for_arm_v1(arm);
            assert_eq!(
                run.record().contracts().trainer_v4_candidate.is_some(),
                arm.uses_baseline_v4_v1()
            );
        }
    }

    #[test]
    fn a_run_without_the_cycle4_section_is_rejected_v1() {
        let bytes = crate::native_training_store_run_v2::test_fixture_bytes_v2();
        let run = decode_train_run_v2(&bytes).expect("v3 fixture validates");
        let error = validate_run_contract_v1(&run, Cycle4ArmKindV1::ControlR)
            .expect_err("a v3 run is not a cycle-4 arm");
        assert_eq!(
            error.code_v1(),
            "cycle4_arm_v1_missing_population_program_v2_cycle4"
        );
    }

    #[test]
    fn v4_arms_require_the_cuda_burn_dense_device_contract_v1() {
        for arm in [Cycle4ArmKindV1::StaticRb, Cycle4ArmKindV1::TreatmentRb] {
            let run = run_for_arm_v1(arm);
            let backend =
                validate_device_contract_v1(&run, arm).expect("v4 fixtures bind CudaBurnDense");
            assert_eq!(backend, NativeTrainingNumericalBackendV1::CudaBurnDense);
        }
        let control = run_for_arm_v1(Cycle4ArmKindV1::ControlR);
        let backend = validate_device_contract_v1(&control, Cycle4ArmKindV1::ControlR)
            .expect("control-r keeps its own frozen backend");
        assert_ne!(backend, NativeTrainingNumericalBackendV1::CudaBurnDense);
    }

    #[test]
    fn execution_config_is_derived_from_the_run_record_v1() {
        let run = run_for_arm_v1(Cycle4ArmKindV1::ControlR);
        let backend =
            validate_device_contract_v1(&run, Cycle4ArmKindV1::ControlR).expect("backend");
        let config = execution_config_from_run_v1(&run, backend).expect("config");
        // The Store's own producer-side validator is the authority on this
        // config, so proving it accepts what we derived proves the derivation.
        crate::native_training_store_update_group_v1::validate_prepared_execution_config_v1(
            &run, &config,
        )
        .expect("derived execution config must satisfy the run's own binding");
    }

    // ------------------------------------------------------------------
    // Origin record: verify-or-publish, and the bootstrap adopt rule
    // ------------------------------------------------------------------

    #[test]
    fn the_origin_record_is_published_when_the_chain_has_none_v1() {
        let dir = fresh_temp_dir_v1("origin-publish");
        let run = seeded_run_for_arm_v1(Cycle4ArmKindV1::TreatmentRb);
        let identity = genesis_identity_fixture_v1(1);
        let path = dir.join(CYCLE4_ARM_ORIGIN_RECORD_FILENAME_V1);
        assert!(!path.exists());
        ensure_origin_record_v1(&dir, Cycle4ArmKindV1::TreatmentRb, &run, &identity)
            .expect("a missing origin record is published");
        assert!(path.is_file(), "the record must use the contract's name");
        let decoded: Cycle4ArmOriginRecordV1 = from_canonical_json_bytes_v1(
            &std::fs::read(&path).expect("read"),
            CanonicalJsonNullPolicyV1::Forbid,
        )
        .expect("the published record decodes");
        assert_eq!(decoded.schema, CYCLE4_ARM_ORIGIN_RECORD_SCHEMA_V1);
        assert_eq!(decoded.run_sha256, run.run_sha256());
        assert_eq!(decoded.arm_kind, Cycle4ArmKindV1::TreatmentRb.wire_v1());
        assert_eq!(
            decoded.genesis_checkpoint_manifest_sha256,
            identity.checkpoint_manifest_sha256
        );
        assert_eq!(
            decoded.genesis_train_state_sha256,
            identity.train_state_sha256
        );
        // Verify-or-publish is idempotent: the second open reads it back and
        // agrees, leaving the bytes untouched.
        let before = std::fs::read(&path).expect("read");
        ensure_origin_record_v1(&dir, Cycle4ArmKindV1::TreatmentRb, &run, &identity)
            .expect("an agreeing origin record is accepted");
        assert_eq!(std::fs::read(&path).expect("read"), before);
        assert!(
            !dir.read_dir()
                .expect("read dir")
                .filter_map(std::result::Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().contains(".tmp-")),
            "no temporary file may survive a publication"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_origin_record_that_binds_something_else_fails_closed_v1() {
        let dir = fresh_temp_dir_v1("origin-conflict");
        let run = seeded_run_for_arm_v1(Cycle4ArmKindV1::TreatmentRb);
        let identity = genesis_identity_fixture_v1(1);
        ensure_origin_record_v1(&dir, Cycle4ArmKindV1::TreatmentRb, &run, &identity)
            .expect("publish");
        // A different genesis checkpoint under the same run and arm.
        let error = ensure_origin_record_v1(
            &dir,
            Cycle4ArmKindV1::TreatmentRb,
            &run,
            &genesis_identity_fixture_v1(2),
        )
        .expect_err("a different genesis checkpoint is a conflict");
        assert_eq!(error.failure_v1(), Cycle4ArmFailureV1::Contract);
        assert_eq!(error.code_v1(), "cycle4_arm_v1_origin_record_conflict");
        // A different arm kind on the same chain directory.
        let error = ensure_origin_record_v1(&dir, Cycle4ArmKindV1::ControlR, &run, &identity)
            .expect_err("a different arm is a conflict");
        assert_eq!(error.code_v1(), "cycle4_arm_v1_origin_record_conflict");
        // A different run identity.
        let other_run = seeded_run_for_arm_v1(Cycle4ArmKindV1::ControlR);
        assert_ne!(other_run.run_sha256(), run.run_sha256());
        let error = ensure_origin_record_v1(&dir, Cycle4ArmKindV1::ControlR, &other_run, &identity)
            .expect_err("a different run is a conflict");
        assert_eq!(error.code_v1(), "cycle4_arm_v1_origin_record_conflict");
        // Bytes that are not an origin record at all.
        let path = dir.join(CYCLE4_ARM_ORIGIN_RECORD_FILENAME_V1);
        std::fs::write(&path, b"{}").expect("write");
        let error = ensure_origin_record_v1(&dir, Cycle4ArmKindV1::TreatmentRb, &run, &identity)
            .expect_err("an undecodable record is a conflict");
        assert_eq!(error.code_v1(), "cycle4_arm_v1_origin_record_conflict");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_run_without_a_pinned_origin_cannot_have_an_origin_record_v1() {
        // The unseeded fixture declares no `opponent_ladder_initialization`,
        // so there is nothing to restate; every arm kind fails closed.
        let dir = fresh_temp_dir_v1("origin-unpinned");
        for arm in [
            Cycle4ArmKindV1::ControlR,
            Cycle4ArmKindV1::StaticRb,
            Cycle4ArmKindV1::TreatmentRb,
        ] {
            let run = run_for_arm_v1(arm);
            let error = ensure_origin_record_v1(&dir, arm, &run, &genesis_identity_fixture_v1(3))
                .expect_err("no pinned parent, no origin record");
            assert_eq!(error.code_v1(), "cycle4_arm_v1_genesis_requires_origin");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_bootstrap_retry_adopts_a_genesis_only_store_and_refuses_a_trained_one_v1() {
        // Review finding: the Store commit and the origin-record write are
        // two steps. A process that died between them leaves a
        // generation-0 Store with no origin record, and refusing that
        // stranded the root, because the wrapper then skips bootstrap
        // (genesis exists) while every retry is rejected.
        let store_root = PathBuf::from("D:/cycle4/arm-a/store");
        bootstrap_may_adopt_seeded_store_v1(0, &store_root)
            .expect("a genesis-only Store is exactly what an interrupted bootstrap leaves");
        for generation in [1_u64, 4, 128, 2048] {
            let error = bootstrap_may_adopt_seeded_store_v1(generation, &store_root)
                .expect_err("a trained Store is never adopted by a bootstrap");
            assert_eq!(error.failure_v1(), Cycle4ArmFailureV1::Contract);
            assert_eq!(error.exit_code_v1(), 3);
            assert_eq!(error.code_v1(), "cycle4_arm_v1_genesis_already_present");
            assert!(error.detail_v1().contains(&generation.to_string()));
        }
    }

    #[test]
    fn a_bootstrap_retry_publishes_the_origin_record_the_lost_process_owed_v1() {
        // The adopt path reads the four genesis hashes back off the Store's
        // own generation-0 checkpoint and finishes the publication. Modelled
        // here at the two steps a Store hands over: the identity it reports,
        // and the record that identity implies.
        let dir = fresh_temp_dir_v1("origin-retry");
        let run = seeded_run_for_arm_v1(Cycle4ArmKindV1::ControlR);
        let identity = genesis_identity_fixture_v1(7);
        let path = dir.join(CYCLE4_ARM_ORIGIN_RECORD_FILENAME_V1);
        assert!(!path.exists(), "the lost process never wrote it");
        bootstrap_may_adopt_seeded_store_v1(0, Path::new("D:/cycle4/arm-a/store"))
            .expect("adoptable");
        ensure_origin_record_v1(&dir, Cycle4ArmKindV1::ControlR, &run, &identity)
            .expect("the retry finishes the publication");
        assert!(path.is_file());
        // And the interval path that follows agrees with what it published.
        ensure_origin_record_v1(&dir, Cycle4ArmKindV1::ControlR, &run, &identity)
            .expect("the first interval verifies the same record");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ------------------------------------------------------------------
    // Slot locator and roster binding
    // ------------------------------------------------------------------

    #[test]
    fn locator_resolves_slot_roots_in_roster_order_v1() {
        let manifest = genesis_manifest_v1();
        let locator = locator_for_v1(&manifest);
        let roots = slot_store_roots_for_manifest_v1(&locator, &manifest).expect("resolve");
        assert_eq!(roots.len(), CYCLE4_SLOT_COUNT_V1);
        for (index, root) in roots.iter().enumerate() {
            assert_eq!(root, &PathBuf::from(format!("D:/cycle4/slot-{index}")));
        }
    }

    #[test]
    fn locator_roster_mismatch_fails_closed_v1() {
        let manifest = genesis_manifest_v1();
        let mut locator = locator_for_v1(&manifest);
        locator.stores[3].checkpoint_manifest_sha256 = digest_v1(7, 7);
        let error = slot_store_roots_for_manifest_v1(&locator, &manifest)
            .expect_err("a substituted identity must fail closed");
        assert_eq!(error.failure_v1(), Cycle4ArmFailureV1::Contract);
        assert_eq!(
            error.code_v1(),
            "cycle4_arm_v1_slot_locator_roster_mismatch"
        );
    }

    #[test]
    fn locator_rejects_wrong_schema_count_duplicates_and_relative_paths_v1() {
        let manifest = genesis_manifest_v1();
        let encode = |locator: &Cycle4SlotLocatorV1| serde_json::to_vec(locator).expect("encode");

        let mut wrong_schema = locator_for_v1(&manifest);
        wrong_schema.schema = "mtg-kernel-cycle4-slot-locator/v1".to_owned();
        assert_eq!(
            decode_slot_locator_v1(&encode(&wrong_schema))
                .expect_err("schema")
                .code_v1(),
            "cycle4_arm_v1_slot_locator_schema"
        );

        let mut short = locator_for_v1(&manifest);
        short.stores.pop();
        assert_eq!(
            decode_slot_locator_v1(&encode(&short))
                .expect_err("count")
                .code_v1(),
            "cycle4_arm_v1_slot_locator_slot_count"
        );

        let mut duplicate = locator_for_v1(&manifest);
        duplicate.stores[1].checkpoint_manifest_sha256 =
            duplicate.stores[0].checkpoint_manifest_sha256.clone();
        assert_eq!(
            decode_slot_locator_v1(&encode(&duplicate))
                .expect_err("duplicate")
                .code_v1(),
            "cycle4_arm_v1_slot_locator_duplicate_identity"
        );

        let mut relative = locator_for_v1(&manifest);
        relative.stores[2].store_root = "slots/two".to_owned();
        assert_eq!(
            decode_slot_locator_v1(&encode(&relative))
                .expect_err("relative")
                .code_v1(),
            "cycle4_arm_v1_slot_locator_relative_path"
        );

        let mut malformed_identity = locator_for_v1(&manifest);
        malformed_identity.stores[4].checkpoint_manifest_sha256 = "not-a-digest".to_owned();
        assert_eq!(
            decode_slot_locator_v1(&encode(&malformed_identity))
                .expect_err("identity")
                .code_v1(),
            "cycle4_arm_v1_slot_locator_identity"
        );

        let good = locator_for_v1(&manifest);
        decode_slot_locator_v1(&encode(&good)).expect("a well-formed locator decodes");
    }

    // ------------------------------------------------------------------
    // Own-run generation translation (trainee-local label -> Store)
    // ------------------------------------------------------------------

    #[test]
    fn own_run_slot_labels_translate_by_the_896_offset_v1() {
        // The manifest labels every slot trainee-locally; the arm's own
        // Store counts 0..=2048 for 896..=2944, so an own-run label of
        // `896 + n` is read at Store generation `n`.
        let arm_run = foreign_run_sha256_v1();
        for refresh_index in [0_u64, 1, 4, 16] {
            let slots = manifest_slots_v1(refresh_index, &arm_run, FOREIGN_BASE_SEED_V1);
            let program_update = refresh_index * CYCLE4_REFRESH_INTERVAL_V1;
            // current-1 is own-run at every index.
            let current_1 = &slots[5];
            assert_eq!(
                current_1.source_generation,
                CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 + program_update,
                "the manifest label stays trainee-local"
            );
            assert_eq!(
                store_generation_for_slot_v1(current_1, &arm_run).expect("current-1 translates"),
                program_update,
                "the Store is read at the translated generation"
            );
        }
    }

    #[test]
    fn historical_zero_translates_only_once_it_is_own_run_v1() {
        let arm_run = foreign_run_sha256_v1();
        // Indices 0..=3 read the cycle-3 lineage store, an OTHER run: its
        // own numbering applies and nothing is translated.
        for refresh_index in 0..=3_u64 {
            let slots = manifest_slots_v1(refresh_index, &arm_run, FOREIGN_BASE_SEED_V1);
            let historical_0 = &slots[2];
            assert_eq!(
                historical_0.source_run_sha256,
                CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1
            );
            let label = CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1
                + refresh_index * CYCLE4_REFRESH_INTERVAL_V1
                - CYCLE4_HISTORICAL_LAG_V1;
            assert_eq!(historical_0.source_generation, label);
            assert_eq!(
                store_generation_for_slot_v1(historical_0, &arm_run).expect("other run"),
                label,
                "an other-run slot is read at its label verbatim"
            );
        }
        // From index 4 it is the arm's own store, so the same lag resolves
        // at the translated generation.
        let slots = manifest_slots_v1(4, &arm_run, FOREIGN_BASE_SEED_V1);
        let historical_0 = &slots[2];
        assert_eq!(historical_0.source_run_sha256, arm_run);
        assert_eq!(
            historical_0.source_generation,
            CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 + 4 * CYCLE4_REFRESH_INTERVAL_V1
                - CYCLE4_HISTORICAL_LAG_V1
        );
        assert_eq!(
            store_generation_for_slot_v1(historical_0, &arm_run).expect("own run"),
            4 * CYCLE4_REFRESH_INTERVAL_V1 - CYCLE4_HISTORICAL_LAG_V1,
            "historical-0 at refresh 4 is read at store generation 0"
        );
    }

    #[test]
    fn every_other_run_slot_keeps_its_own_lineage_numbering_v1() {
        // The anchors, the historical-1 rotation, current-0, and the two
        // frozen fallbacks all name completed runs that number their own
        // stores; translation must never touch them.
        let arm_run = foreign_run_sha256_v1();
        let slots = manifest_slots_v1(1, &arm_run, FOREIGN_BASE_SEED_V1);
        for index in [0_usize, 1, 3, 4, 6, 7] {
            let slot = &slots[index];
            assert_ne!(slot.source_run_sha256, arm_run);
            assert_eq!(
                store_generation_for_slot_v1(slot, &arm_run).expect("other run"),
                slot.source_generation
            );
        }
        assert_eq!(
            slots[0].source_generation,
            CYCLE4_ANCHOR_0_V1.source_generation
        );
        assert_eq!(
            slots[1].source_generation,
            CYCLE4_ANCHOR_1_V1.source_generation
        );
        assert_eq!(
            slots[4].source_generation,
            CYCLE4_CURRENT_0_V1.source_generation
        );
    }

    #[test]
    fn an_own_run_label_below_the_program_start_fails_closed_v1() {
        let arm_run = foreign_run_sha256_v1();
        let mut slots = manifest_slots_v1(1, &arm_run, FOREIGN_BASE_SEED_V1);
        for label in [0_u64, 1, CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 - 1] {
            slots[5].source_generation = label;
            let error = store_generation_for_slot_v1(&slots[5], &arm_run)
                .expect_err("no store generation can carry this label");
            assert_eq!(error.failure_v1(), Cycle4ArmFailureV1::Contract);
            assert_eq!(error.code_v1(), "cycle4_arm_v1_own_run_slot_generation");
        }
        // The boundary case is admissible: 896 is the arm's own genesis.
        slots[5].source_generation = CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1;
        assert_eq!(
            store_generation_for_slot_v1(&slots[5], &arm_run).expect("the program start"),
            0
        );
    }

    #[test]
    fn a_store_without_the_translated_generation_fails_closed_v1() {
        // Every slot is translated before a single file is opened, and the
        // Store is then asked for exactly the translated generation. A store
        // that cannot supply it never yields an occupant: resolution fails
        // closed rather than falling back to the label or to another
        // generation.
        let arm_run = foreign_run_sha256_v1();
        let genesis = genesis_manifest_v1();
        let roots: Vec<PathBuf> = (0..CYCLE4_SLOT_COUNT_V1)
            .map(|index| PathBuf::from(format!("D:/cycle4/absent-slot-{index}")))
            .collect();
        let error = resolve_population_opponent_cycle4_v1(&genesis, &roots, &arm_run, None)
            .expect_err("a store that carries nothing cannot occupy a slot");
        assert_eq!(error.code_v1(), "cycle4_arm_v1_population_run_read");
    }

    // ------------------------------------------------------------------
    // Refresh manifest decode and the static-pool rule
    // ------------------------------------------------------------------

    fn write_chain_v1(dir: &Path, genesis: &Cycle4RefreshManifestV1) -> PathBuf {
        let path = dir.join(cycle4_chain_manifest_filename_v1(0));
        std::fs::write(&path, genesis.canonical_bytes_v1()).expect("write genesis");
        path
    }

    #[test]
    fn genesis_manifest_decodes_without_a_panel_v1() {
        let dir = fresh_temp_dir_v1("genesis-decode");
        let genesis = genesis_manifest_v1();
        let path = write_chain_v1(&dir, &genesis);
        let decoded = decode_interval_manifest_v1(&path, None).expect("decode");
        assert_eq!(decoded.refresh_index_v1(), 0);
        assert_eq!(decoded.manifest_sha256_v1(), genesis.manifest_sha256_v1());
        let error =
            decode_interval_manifest_v1(&path, Some(&path)).expect_err("genesis binds no panel");
        assert_eq!(error.code_v1(), "cycle4_arm_v1_genesis_takes_no_panel");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_genesis_manifest_requires_its_panel_bytes_v1() {
        let dir = fresh_temp_dir_v1("panel-binding");
        let genesis = genesis_manifest_v1();
        write_chain_v1(&dir, &genesis);
        let panel =
            br#"{"schema":"mtg-kernel-cycle4-payoff-panel/v1","rank_sums":[0,0,0,0,0,0,0,0]}"#;
        let refresh_one = refresh_one_manifest_v1(&genesis, panel);
        let manifest_path = dir.join(cycle4_chain_manifest_filename_v1(1));
        std::fs::write(&manifest_path, refresh_one.canonical_bytes_v1()).expect("write");
        let panel_path = dir.join(cycle4_chain_panel_filename_v1(1));
        std::fs::write(&panel_path, panel).expect("write panel");

        let decoded =
            decode_interval_manifest_v1(&manifest_path, Some(&panel_path)).expect("decode");
        assert_eq!(decoded.refresh_index_v1(), 1);

        let missing = decode_interval_manifest_v1(&manifest_path, None)
            .expect_err("a non-genesis manifest needs its panel");
        assert_eq!(missing.code_v1(), "cycle4_arm_v1_missing_payoff_panel");

        let wrong_panel_path = dir.join("wrong.panel.json");
        std::fs::write(&wrong_panel_path, b"not the panel").expect("write");
        let mismatch = decode_interval_manifest_v1(&manifest_path, Some(&wrong_panel_path))
            .expect_err("panel content must resolve");
        assert_eq!(
            mismatch.code_v1(),
            "cycle4_arm_v1_refresh_manifest_rejected"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A genesis and a refresh-1 manifest bound to `run`'s own trainee
    /// identity, so the manifest/run binding gate passes and later gates are
    /// the ones under test.
    fn bound_manifests_v1(
        run: &ValidatedTrainRunV2,
    ) -> (Cycle4RefreshManifestV1, Cycle4RefreshManifestV1) {
        let base_seed = run.record().schedule().base_seed;
        let genesis = genesis_manifest_for_v1(run.run_sha256(), base_seed);
        let refresh_one = refresh_one_manifest_for_v1(
            &genesis,
            b"panel-bytes-for-refresh-one",
            run.run_sha256(),
            base_seed,
        );
        (genesis, refresh_one)
    }

    #[test]
    fn static_rb_refuses_a_non_genesis_manifest_v1() {
        let run = run_for_arm_v1(Cycle4ArmKindV1::StaticRb);
        let (genesis, refresh_one) = bound_manifests_v1(&run);
        // The genesis manifest is the only one STATIC-RB ever accepts.
        validate_manifest_against_run_v1(&genesis, &run, Cycle4ArmKindV1::StaticRb)
            .expect("static-rb accepts its frozen genesis manifest");
        let error = validate_manifest_against_run_v1(&refresh_one, &run, Cycle4ArmKindV1::StaticRb)
            .expect_err("static-rb never advances past genesis");
        assert_eq!(
            error.code_v1(),
            "cycle4_arm_v1_static_pool_manifest_advanced"
        );
    }

    #[test]
    fn refresh_chained_arms_accept_an_advanced_manifest_v1() {
        for arm in [Cycle4ArmKindV1::ControlR, Cycle4ArmKindV1::TreatmentRb] {
            let run = run_for_arm_v1(arm);
            let (genesis, refresh_one) = bound_manifests_v1(&run);
            let at_genesis =
                validate_manifest_against_run_v1(&genesis, &run, arm).expect("genesis accepted");
            assert_eq!(at_genesis.program_update, 0);
            assert_eq!(
                at_genesis.trainee_local_generation,
                CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1
            );
            let at_one =
                validate_manifest_against_run_v1(&refresh_one, &run, arm).expect("refresh 1");
            assert_eq!(at_one.refresh_index, 1);
            assert_eq!(at_one.program_update, CYCLE4_REFRESH_INTERVAL_V1);
            assert_eq!(
                at_one.trainee_local_generation,
                CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 + CYCLE4_REFRESH_INTERVAL_V1
            );
        }
    }

    #[test]
    fn manifest_must_bind_this_run_identity_v1() {
        let genesis = genesis_manifest_v1();
        let run = run_for_arm_v1(Cycle4ArmKindV1::ControlR);
        let error = validate_manifest_against_run_v1(&genesis, &run, Cycle4ArmKindV1::ControlR)
            .expect_err("a manifest bound to another trainee must fail closed");
        assert_eq!(error.code_v1(), "cycle4_arm_v1_manifest_run_binding");
    }

    // ------------------------------------------------------------------
    // Interval stop
    // ------------------------------------------------------------------

    fn contract_at_v1(refresh_index: u64) -> Cycle4ArmContractV1 {
        let program_update = refresh_index * CYCLE4_REFRESH_INTERVAL_V1;
        Cycle4ArmContractV1 {
            refresh_index,
            program_update,
            trainee_local_generation: CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 + program_update,
        }
    }

    #[test]
    fn interval_stop_is_exactly_one_refresh_interval_v1() {
        let contract = contract_at_v1(2);
        validate_interval_stop_v1(384, 256, 4, &contract, Cycle4ArmKindV1::TreatmentRb, None)
            .expect("256 + 128 == 384");
        for stop in [383_u64, 385, 512] {
            assert_eq!(
                validate_interval_stop_v1(
                    stop,
                    256,
                    4,
                    &contract,
                    Cycle4ArmKindV1::TreatmentRb,
                    None
                )
                .expect_err("only one interval per process")
                .code_v1(),
                "cycle4_arm_v1_interval_stop"
            );
        }
        // The stop names the whole interval, so it is always a multiple of
        // the pre-registered 128; anything else names no interval at all.
        for stop in [0_u64, 64, 127, 129, 200] {
            assert_eq!(
                validate_interval_stop_v1(
                    stop,
                    0,
                    4,
                    &contract_at_v1(0),
                    Cycle4ArmKindV1::TreatmentRb,
                    None
                )
                .expect_err("not a whole refresh interval")
                .code_v1(),
                "cycle4_arm_v1_interval_stop"
            );
        }
    }

    #[test]
    fn a_completed_interval_resumes_idempotently_at_its_stop_v1() {
        // Review finding P1: a process that committed this interval's final
        // generation and died before returning resumes exactly at the stop.
        // That is a validated completion, not a contract violation: the
        // caller breaks immediately, revalidates the whole Store, and
        // returns the outcome the lost process would have returned.
        let contract = contract_at_v1(1);
        validate_interval_stop_v1(256, 256, 4, &contract, Cycle4ArmKindV1::TreatmentRb, None)
            .expect("refresh 1 opens 128 and stops at 256");
        // The ordinary start position for the same interval still passes.
        validate_interval_stop_v1(256, 128, 4, &contract, Cycle4ArmKindV1::TreatmentRb, None)
            .expect("128 + 128 == 256");
        // Both positions are judged against the same manifest: resuming at
        // the stop of an interval this manifest does not open is still a
        // position mismatch.
        assert_eq!(
            validate_interval_stop_v1(
                256,
                256,
                4,
                &contract_at_v1(2),
                Cycle4ArmKindV1::TreatmentRb,
                None
            )
            .expect_err("manifest 2 opens 256, not 128")
            .code_v1(),
            "cycle4_arm_v1_resume_position_mismatch"
        );
        // A position between the start and the stop is an interrupted
        // attempt resuming, which this same manifest and stop still cover.
        validate_interval_stop_v1(256, 192, 4, &contract, Cycle4ArmKindV1::TreatmentRb, None)
            .expect("an interrupted attempt resumes mid-interval");
    }

    #[test]
    fn an_interrupted_interval_resumes_anywhere_inside_it_v1() {
        // The wrapper restarts an interrupted attempt against the SAME stop
        // it was given, so the Store it reopens sits at whatever checkpoint
        // boundary that attempt reached. Interval 384..=512 is refresh 3's.
        let contract = contract_at_v1(3);
        let segment = 4_u64;
        for resume in [384_u64, 388, 392, 508, 512] {
            validate_interval_stop_v1(
                512,
                resume,
                segment,
                &contract,
                Cycle4ArmKindV1::TreatmentRb,
                None,
            )
            .unwrap_or_else(|error| {
                panic!("resume {resume} is inside the interval this stop names: {error}")
            });
        }
        // Off a checkpoint-segment boundary: the Store never publishes such a
        // generation, so naming one is a contract error, not a near miss.
        for resume in [385_u64, 386, 387, 511] {
            assert_eq!(
                validate_interval_stop_v1(
                    512,
                    resume,
                    segment,
                    &contract,
                    Cycle4ArmKindV1::TreatmentRb,
                    None
                )
                .expect_err("not a checkpoint segment boundary")
                .code_v1(),
                "cycle4_arm_v1_interval_stop"
            );
        }
        // Before the interval's start, and past its stop.
        for resume in [0_u64, 256, 380, 516, 640] {
            assert_eq!(
                validate_interval_stop_v1(
                    512,
                    resume,
                    segment,
                    &contract,
                    Cycle4ArmKindV1::TreatmentRb,
                    None
                )
                .expect_err("outside the interval this stop names")
                .code_v1(),
                "cycle4_arm_v1_interval_stop"
            );
        }
        // The manifest rule still binds the interval's START, so a
        // mid-interval resume under the wrong manifest is still rejected.
        assert_eq!(
            validate_interval_stop_v1(
                512,
                388,
                segment,
                &contract_at_v1(4),
                Cycle4ArmKindV1::TreatmentRb,
                None
            )
            .expect_err("refresh 4 opens 512, not 384")
            .code_v1(),
            "cycle4_arm_v1_resume_position_mismatch"
        );
    }

    #[test]
    fn interval_stop_never_passes_the_program_end_v1() {
        let contract = contract_at_v1(CYCLE4_REFRESH_MAX_INDEX_V1);
        assert_eq!(
            validate_interval_stop_v1(2176, 2048, 4, &contract, Cycle4ArmKindV1::TreatmentRb, None)
                .expect_err("2048 is the program end")
                .code_v1(),
            "cycle4_arm_v1_interval_stop"
        );
        // The last TRAINED interval is the one refresh 15 opens (refresh 16
        // is the final panel boundary, not an interval start), and it is
        // admissible from either position.
        let final_interval = contract_at_v1(CYCLE4_REFRESH_MAX_INDEX_V1 - 1);
        validate_interval_stop_v1(
            2048,
            1920,
            4,
            &final_interval,
            Cycle4ArmKindV1::TreatmentRb,
            None,
        )
        .expect("the final interval ends exactly at the program end");
        validate_interval_stop_v1(
            2048,
            2048,
            4,
            &final_interval,
            Cycle4ArmKindV1::TreatmentRb,
            None,
        )
        .expect("a completed final interval resumes idempotently");
    }

    #[test]
    fn refresh_chained_arms_must_resume_at_the_manifest_position_v1() {
        let contract = contract_at_v1(2);
        assert_eq!(
            validate_interval_stop_v1(256, 128, 4, &contract, Cycle4ArmKindV1::TreatmentRb, None)
                .expect_err("manifest 2 opens generation 256, not 128")
                .code_v1(),
            "cycle4_arm_v1_resume_position_mismatch"
        );
        // STATIC-RB reuses the genesis manifest at every interval, so its
        // resume position is not derivable from the manifest and is not
        // checked against it.
        let genesis_contract = contract_at_v1(0);
        validate_interval_stop_v1(
            256,
            128,
            4,
            &genesis_contract,
            Cycle4ArmKindV1::StaticRb,
            None,
        )
        .expect("static-rb intervals are not manifest-positioned");
    }

    // ------------------------------------------------------------------
    // Bounded preflight provision
    // ------------------------------------------------------------------

    #[test]
    fn preflight_relaxes_the_interval_to_exactly_n_updates_v1() {
        let contract = contract_at_v1(0);
        for (updates, segment) in [(2_u64, 2_u64), (4, 4), (8, 8), (8, 4), (4, 2)] {
            validate_interval_stop_v1(
                updates,
                0,
                segment,
                &contract,
                Cycle4ArmKindV1::ControlR,
                Some(updates),
            )
            .expect("stop == resume + n is the relaxed check");
        }
        // Still exact: neither the pre-registered 128 nor any other stop is
        // admissible once a preflight window is declared.
        for stop in [0_u64, 3, 5, 128] {
            assert_eq!(
                validate_interval_stop_v1(
                    stop,
                    0,
                    4,
                    &contract,
                    Cycle4ArmKindV1::ControlR,
                    Some(4)
                )
                .expect_err("only resume + n")
                .code_v1(),
                "cycle4_arm_v1_interval_stop"
            );
        }
    }

    #[test]
    fn preflight_updates_are_bounded_to_one_through_eight_v1() {
        let contract = contract_at_v1(0);
        for updates in [0_u64, 9, 16, 128] {
            assert_eq!(
                validate_interval_stop_v1(
                    updates,
                    0,
                    1,
                    &contract,
                    Cycle4ArmKindV1::ControlR,
                    Some(updates)
                )
                .expect_err("outside 1..=8")
                .code_v1(),
                "cycle4_arm_v1_preflight_updates_range"
            );
        }
    }

    #[test]
    fn a_preflight_window_must_be_a_whole_number_of_checkpoint_segments_v1() {
        let contract = contract_at_v1(0);
        // Segment 4 cannot land exactly on a 2-update stop.
        assert_eq!(
            validate_interval_stop_v1(2, 0, 4, &contract, Cycle4ArmKindV1::ControlR, Some(2))
                .expect_err("2 is not a multiple of 4")
                .code_v1(),
            "cycle4_arm_v1_interval_stop"
        );
        // A segment larger than the whole bound leaves no admissible window,
        // which is a legible rejection rather than a silent overshoot.
        assert_eq!(
            validate_interval_stop_v1(8, 0, 16, &contract, Cycle4ArmKindV1::ControlR, Some(8))
                .expect_err("16 > 8")
                .code_v1(),
            "cycle4_arm_v1_interval_stop"
        );
    }

    #[test]
    fn a_preflight_never_leaves_the_genesis_interval_or_manifest_v1() {
        assert_eq!(
            validate_interval_stop_v1(
                136,
                128,
                4,
                &contract_at_v1(1),
                Cycle4ArmKindV1::ControlR,
                Some(8)
            )
            .expect_err("refresh 1 is not the genesis manifest")
            .code_v1(),
            "cycle4_arm_v1_preflight_manifest_advanced"
        );
        assert_eq!(
            validate_interval_stop_v1(
                136,
                128,
                4,
                &contract_at_v1(0),
                Cycle4ArmKindV1::ControlR,
                Some(8)
            )
            .expect_err("136 is past the first refresh boundary")
            .code_v1(),
            "cycle4_arm_v1_preflight_manifest_advanced"
        );
        // Successive short windows inside the genesis interval are the
        // ladder's own shape and stay admissible.
        validate_interval_stop_v1(
            16,
            8,
            8,
            &contract_at_v1(0),
            Cycle4ArmKindV1::ControlR,
            Some(8),
        )
        .expect("a second short window inside the genesis interval");
    }

    #[test]
    fn the_mode_marker_pins_a_store_prefix_to_one_training_mode_v1() {
        let root = fresh_temp_dir_v1("mode-marker");
        let run = run_for_arm_v1(Cycle4ArmKindV1::ControlR);
        let prefix = root.join("prefix");

        // A preflight claims a fresh prefix, and re-claiming it in the same
        // mode is idempotent.
        claim_store_mode_marker_v1(
            &prefix,
            Cycle4ArmKindV1::ControlR,
            &run,
            Cycle4ArmStoreModeV1::Preflight,
        )
        .expect("first claim");
        claim_store_mode_marker_v1(
            &prefix,
            Cycle4ArmKindV1::ControlR,
            &run,
            Cycle4ArmStoreModeV1::Preflight,
        )
        .expect("same mode re-entry");
        // The formal path may not re-enter a prefix a preflight trained.
        assert_eq!(
            claim_store_mode_marker_v1(
                &prefix,
                Cycle4ArmKindV1::ControlR,
                &run,
                Cycle4ArmStoreModeV1::Formal
            )
            .expect_err("formal may not adopt a preflight prefix")
            .code_v1(),
            "cycle4_arm_v1_mode_marker_conflict"
        );

        // And the reverse: a preflight is refused on a formal prefix.
        let formal = root.join("formal");
        claim_store_mode_marker_v1(
            &formal,
            Cycle4ArmKindV1::ControlR,
            &run,
            Cycle4ArmStoreModeV1::Formal,
        )
        .expect("formal claim");
        assert_eq!(
            claim_store_mode_marker_v1(
                &formal,
                Cycle4ArmKindV1::ControlR,
                &run,
                Cycle4ArmStoreModeV1::Preflight
            )
            .expect_err("a formal marker forbids the relaxed check")
            .code_v1(),
            "cycle4_arm_v1_mode_marker_conflict"
        );
        // A different arm on the same prefix is refused too.
        assert_eq!(
            claim_store_mode_marker_v1(
                &formal,
                Cycle4ArmKindV1::TreatmentRb,
                &run,
                Cycle4ArmStoreModeV1::Formal
            )
            .expect_err("one prefix, one arm")
            .code_v1(),
            "cycle4_arm_v1_mode_marker_conflict"
        );
        // A bootstrapped prefix may not be downgraded back to bootstrap.
        assert_eq!(
            claim_store_mode_marker_v1(
                &formal,
                Cycle4ArmKindV1::ControlR,
                &run,
                Cycle4ArmStoreModeV1::Bootstrap
            )
            .expect_err("formal is terminal")
            .code_v1(),
            "cycle4_arm_v1_mode_marker_conflict"
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_bootstrapped_prefix_may_still_become_either_training_mode_v1() {
        let root = fresh_temp_dir_v1("mode-marker-bootstrap");
        let run = run_for_arm_v1(Cycle4ArmKindV1::ControlR);

        // Nothing has trained after a bootstrap, so neither transition can
        // have been made under the relaxed interval check.
        for (name, mode) in [
            ("to-formal", Cycle4ArmStoreModeV1::Formal),
            ("to-preflight", Cycle4ArmStoreModeV1::Preflight),
        ] {
            let prefix = root.join(name);
            claim_store_mode_marker_v1(
                &prefix,
                Cycle4ArmKindV1::ControlR,
                &run,
                Cycle4ArmStoreModeV1::Bootstrap,
            )
            .expect("bootstrap claim");
            claim_store_mode_marker_v1(&prefix, Cycle4ArmKindV1::ControlR, &run, mode)
                .expect("a bootstrapped prefix admits either training mode");
            // ... but only once: the training mode is then terminal.
            let other = if matches!(mode, Cycle4ArmStoreModeV1::Formal) {
                Cycle4ArmStoreModeV1::Preflight
            } else {
                Cycle4ArmStoreModeV1::Formal
            };
            assert_eq!(
                claim_store_mode_marker_v1(&prefix, Cycle4ArmKindV1::ControlR, &run, other)
                    .expect_err("the training mode is terminal")
                    .code_v1(),
                "cycle4_arm_v1_mode_marker_conflict"
            );
        }

        // A bootstrap for the wrong arm is refused before anything is opened.
        let prefix = root.join("wrong-arm");
        claim_store_mode_marker_v1(
            &prefix,
            Cycle4ArmKindV1::ControlR,
            &run,
            Cycle4ArmStoreModeV1::Bootstrap,
        )
        .expect("bootstrap claim");
        assert_eq!(
            claim_store_mode_marker_v1(
                &prefix,
                Cycle4ArmKindV1::TreatmentRb,
                &run,
                Cycle4ArmStoreModeV1::Formal
            )
            .expect_err("the arm is fixed by the bootstrap")
            .code_v1(),
            "cycle4_arm_v1_mode_marker_conflict"
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn verifying_a_store_mode_marker_never_writes_one_v1() {
        let root = fresh_temp_dir_v1("mode-marker-verify");
        let arm = Cycle4ArmKindV1::ControlR;
        let run = run_for_arm_v1(arm);
        let prefix = root.join("prefix");
        std::fs::create_dir_all(&prefix).expect("create prefix");
        let marker = prefix.join(CYCLE4_ARM_MODE_MARKER_FILENAME_V1);

        // An unclaimed prefix reports Absent and stays unclaimed.
        assert_eq!(
            verify_store_mode_marker_v1(&prefix, arm, &run, Cycle4ArmStoreModeV1::Formal)
                .expect("an unclaimed prefix admits any mode"),
            Cycle4ArmStoreModeStateV1::Absent
        );
        assert!(!marker.exists(), "verification never creates a marker");

        // A bootstrap marker reports promotable, and stays a bootstrap marker
        // until something actually claims it.
        claim_store_mode_marker_v1(&prefix, arm, &run, Cycle4ArmStoreModeV1::Bootstrap)
            .expect("bootstrap claim");
        let before = std::fs::read(&marker).expect("read marker");
        assert_eq!(
            verify_store_mode_marker_v1(&prefix, arm, &run, Cycle4ArmStoreModeV1::Formal)
                .expect("a bootstrap marker is promotable"),
            Cycle4ArmStoreModeStateV1::PromotableFromBootstrap
        );
        assert_eq!(
            std::fs::read(&marker).expect("read marker"),
            before,
            "verification never promotes on its own"
        );
        assert_eq!(
            verify_store_mode_marker_v1(&prefix, arm, &run, Cycle4ArmStoreModeV1::Bootstrap)
                .expect("the same mode is already claimed"),
            Cycle4ArmStoreModeStateV1::AlreadyClaimed
        );

        // A terminal mode is still refused by verification alone, so a caller
        // can fail closed before it opens anything.
        claim_store_mode_marker_v1(&prefix, arm, &run, Cycle4ArmStoreModeV1::Formal)
            .expect("promote to formal");
        assert_eq!(
            verify_store_mode_marker_v1(&prefix, arm, &run, Cycle4ArmStoreModeV1::Preflight)
                .expect_err("formal is terminal")
                .code_v1(),
            "cycle4_arm_v1_mode_marker_conflict"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_interval_on_an_unseeded_prefix_leaves_it_bootstrap_eligible_v1() {
        // Review finding P2: the interval path claimed the TERMINAL formal or
        // preflight marker before it checked for genesis, so an interval
        // pointed at a prefix that had never been bootstrapped stranded it --
        // the run was rejected, but the marker it had already written made the
        // operator's next `--bootstrap-genesis` impossible. A rejected
        // interval must leave the prefix exactly as it found it.
        let root = fresh_temp_dir_v1("interval-unseeded-prefix");
        let arm = Cycle4ArmKindV1::ControlR;
        let run = seeded_run_for_arm_v1(arm);
        let base_seed = run.record().schedule().base_seed;

        // The three plain-file inputs an interval reads before it ever looks
        // at the Store.
        let run_record = root.join("run.json");
        std::fs::write(
            &run_record,
            test_fixture_bytes_population_program_v2_cycle4_seeded_v1(arm.wire_v1()),
        )
        .expect("write run record");
        let refresh_dir = root.join("refresh");
        std::fs::create_dir_all(&refresh_dir).expect("create refresh dir");
        let genesis = genesis_manifest_for_v1(run.run_sha256(), base_seed);
        let manifest_path = write_chain_v1(&refresh_dir, &genesis);
        let locator_path = root.join("slot-locator.json");
        std::fs::write(
            &locator_path,
            serde_json::to_vec(&locator_for_v1(&genesis)).expect("encode locator"),
        )
        .expect("write locator");

        let prefix = root.join("prefix");
        std::fs::create_dir_all(&prefix).expect("create prefix");
        let marker = prefix.join(CYCLE4_ARM_MODE_MARKER_FILENAME_V1);
        let request = Cycle4ArmRequestV1 {
            arm,
            store_root: prefix.join("store"),
            run_record,
            chain_dir: root.join("chain"),
            refresh_manifest: manifest_path,
            payoff_panel: None,
            slot_locator: locator_path,
            stop_generation: CYCLE4_REFRESH_INTERVAL_V1,
            preflight_updates: None,
        };

        let error = run_native_cycle4_arm_v1(&request)
            .expect_err("an unseeded prefix has no interval to run");
        assert_eq!(error.code_v1(), "cycle4_arm_v1_genesis_not_bootstrapped");
        assert_eq!(error.failure_v1(), Cycle4ArmFailureV1::Contract);
        assert_eq!(error.exit_code_v1(), 3);
        assert!(
            !marker.exists(),
            "a rejected interval must not claim the prefix"
        );

        // The prefix is therefore still bootstrap-eligible, and the interval
        // mode can be promoted onto it afterwards exactly as it should be.
        claim_store_mode_marker_v1(&prefix, arm, &run, Cycle4ArmStoreModeV1::Bootstrap)
            .expect("a rejected interval leaves the prefix bootstrap-eligible");
        assert!(marker.is_file());
        claim_store_mode_marker_v1(&prefix, arm, &run, Cycle4ArmStoreModeV1::Formal)
            .expect("the bootstrapped prefix promotes to formal");

        // Repeating the interval is still refused, and now for the same
        // reason as before: the mode marker is not what rejects it.
        let stranded = root.join("stranded");
        claim_store_mode_marker_v1(&stranded, arm, &run, Cycle4ArmStoreModeV1::Formal)
            .expect("claim a formal prefix");
        assert_eq!(
            claim_store_mode_marker_v1(&stranded, arm, &run, Cycle4ArmStoreModeV1::Bootstrap)
                .expect_err("this is what the old ordering left behind")
                .code_v1(),
            "cycle4_arm_v1_mode_marker_conflict"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn store_root_parts_split_parent_and_basename_v1() {
        let (parent, basename) =
            store_root_parts_v1(Path::new("D:/cycle4/arm-a/store")).expect("split");
        assert_eq!(parent, PathBuf::from("D:/cycle4/arm-a"));
        assert_eq!(basename, "store");
    }

    // ------------------------------------------------------------------
    // Head-to-head boundary mode
    // ------------------------------------------------------------------

    #[test]
    fn the_h2h_boundary_mode_gate_pairs_v4_runs_with_chain_dirs_v1() {
        // Review finding P1: after the first interval a treatment-rb or
        // static-rb slot names a TRAINED v4 boundary, which the plain walk
        // rejects. The gate makes that a decision, not a panic.
        assert_eq!(
            cycle4_h2h_boundary_mode_v1(true, Some("D:/cycle4/arm-a/chain")),
            Ok(Cycle4H2hBoundaryModeV1::BaselineV4)
        );
        assert_eq!(
            cycle4_h2h_boundary_mode_v1(false, None),
            Ok(Cycle4H2hBoundaryModeV1::Plain)
        );
        assert_eq!(
            cycle4_h2h_boundary_mode_v1(true, None),
            Err("cycle4_h2h_v4_run_requires_chain_dir"),
            "a v4 run without a chain directory cannot be loaded at all"
        );
        assert_eq!(
            cycle4_h2h_boundary_mode_v1(false, Some("D:/cycle4/arm-a/chain")),
            Err("cycle4_h2h_chain_dir_requires_v4_run"),
            "a chain directory on a v3 run would be silently ignored"
        );
    }

    #[test]
    fn a_probe_access_never_reconstructs_a_missing_sidecar_v1() {
        // The panel probe opens slot Stores read-only. It never reconciles,
        // so nothing authorizes reconstruction and a missing sidecar stays a
        // hard failure rather than being invented during an evaluation.
        let dir = fresh_temp_dir_v1("probe-access");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        for update_index in 0_u64..=8 {
            assert!(!access.may_reconstruct_sidecar_v4(update_index));
        }
        assert!(access.sidecar_record_bytes_v4(1).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    // ------------------------------------------------------------------
    // Baseline chain directory access
    // ------------------------------------------------------------------

    fn cell_key_v1(nonce: u8, role: BaselineRoleV4) -> BaselineCellKeyV4 {
        BaselineCellKeyV4::new_v4(digest_v1(5, nonce), role).expect("cell key")
    }

    /// One synthetic sidecar whose declared successor is exactly what
    /// `apply_update_v4` derives from its own observations.
    fn synthetic_sidecar_v1(
        prior: &NativeBaselineStateV4,
        update_index: u64,
        nonce: u8,
    ) -> Vec<u8> {
        let key = cell_key_v1(nonce, BaselineRoleV4::P0);
        let residual_sum_f64 = f64::from(i32::from(nonce)) * 0.5_f64;
        let observation = BaselineObservationV4 {
            key: key.clone(),
            residual_sum_f64,
            decision_count: 8,
            episode_count: 4,
        };
        let successor = prior
            .apply_update_v4(std::slice::from_ref(&observation))
            .expect("apply");
        let parts = UpdateBaselineRecordPartsV4 {
            update_index,
            update_evidence_sha256: [nonce; 32],
            cells: vec![UpdateBaselineCellPartsV4 {
                opponent_checkpoint_manifest_sha256: key
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: key.role,
                c_t_bits: prior.c_for_cell_v4(&key).to_bits(),
                c_next_bits: successor.c_for_cell_v4(&key).to_bits(),
                residual_sum_f64,
                decision_count: 8,
                episode_count: 4,
            }],
            declared_policy_sum_bits: 0,
        };
        build_update_baseline_record_v4(parts)
            .expect("record")
            .canonical_bytes()
            .to_vec()
    }

    #[test]
    fn sidecar_publication_is_durable_before_the_store_and_idempotent_v1() {
        let dir = fresh_temp_dir_v1("sidecar-publish");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 3);
        let promoted = dir.join("baseline-update-00000001.record.json");
        let staged = dir.join(cycle4_staged_sidecar_name_v1(
            "baseline-update-00000001.record.json",
        ));
        assert!(access.stage_sidecar_record_v4(1, &bytes));
        // Review finding P1: staging is DURABLE and happens before the Store
        // publishes this update, so the Store can never hold v4 evidence
        // whose sidecar exists only in the producing process's memory.
        assert!(staged.is_file(), "a staged sidecar must be on disk at once");
        assert_eq!(std::fs::read(&staged).expect("read"), bytes);
        // It is not yet at its immutable name: that is what says the Store
        // committed the update it explains.
        assert!(
            !promoted.exists(),
            "a staged sidecar must not hold its immutable name yet"
        );
        assert_eq!(access.sidecar_record_bytes_v4(1).expect("staged"), bytes);
        assert!(access.commit_staged_sidecar_records_v4());
        assert!(
            promoted.is_file(),
            "sidecar must use the contract's exact name"
        );
        assert_eq!(std::fs::read(&promoted).expect("read"), bytes);
        assert!(!staged.exists(), "promotion consumes the staged copy");
        // Committing again is a no-op: the staged area is empty.
        assert!(access.commit_staged_sidecar_records_v4());
        // Crash replay of the identical record is accepted; different bytes
        // for the same update are not.
        assert!(access.stage_sidecar_record_v4(1, &bytes));
        assert!(!access.stage_sidecar_record_v4(1, b"different"));
        assert!(
            !dir.read_dir()
                .expect("read dir")
                .filter_map(std::result::Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().contains(".tmp-")),
            "no temporary file may survive a publication"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Stages `1..=count` under `access`, folding the baseline forward the
    /// way a real segment does, and returns the resulting state.
    fn stage_segment_v1(access: &Cycle4BaselineChainAccessV1, count: u64) -> NativeBaselineStateV4 {
        let mut state = NativeBaselineStateV4::empty_v4();
        for update_index in 1..=count {
            let bytes = synthetic_sidecar_v1(&state, update_index, update_index as u8);
            assert!(access.stage_sidecar_record_v4(update_index, &bytes));
            state = access
                .replay_sidecar_v1(&state, update_index)
                .expect("staged bytes replay before promotion");
        }
        state
    }

    #[test]
    fn staged_sidecars_are_published_through_the_durable_move_primitive_v1() {
        // Review finding P1: a synced staging file followed by a plain rename
        // leaves the DIRENT unsynced, so a reboot after the Store commit can
        // lose the name while the committed Store survives, and only the tip
        // is reconstructible. Both the staged write and the promotion must go
        // through the repository's durable move publication, whose receipt
        // names the mechanism that actually ran.
        let dir = fresh_temp_dir_v1("sidecar-durable");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 9);
        let final_name = sidecar_record_name_v4(1).expect("name");
        let staged_name = cycle4_staged_sidecar_name_v1(&final_name);

        let staged_receipt =
            Cycle4BaselineChainAccessV1::publish_sidecar_bytes_v1(&dir, &staged_name, &bytes)
                .expect("the staged record publishes durably");
        assert_eq!(staged_receipt.mechanism(), EXPECTED_DURABLE_MECHANISM_V1);
        // The primitive canonicalizes its parent, which on Windows is an
        // extended-length path, so the receipt is compared canonically.
        assert_eq!(
            staged_receipt.final_path(),
            std::fs::canonicalize(dir.join(&staged_name)).expect("the staged record exists")
        );
        assert_eq!(staged_receipt.sha256(), sha256_v1(&bytes));
        // The primitive leaves no staging debris of its own behind.
        assert!(!dir
            .join(cycle4_sidecar_stage_name_v1(&staged_name))
            .exists());
        // Staging shares the chain directory, so the scan must find exactly
        // the staged record and be untroubled by everything else there.
        std::fs::write(dir.join("arm-origin.record.json"), b"another writer's file")
            .expect("write");
        assert_eq!(access.staged_update_indexes_v1().expect("staged"), vec![1]);

        // Promotion republishes through the same primitive rather than
        // renaming across directories.
        let promoted_receipt =
            Cycle4BaselineChainAccessV1::publish_sidecar_bytes_v1(&dir, &final_name, &bytes)
                .expect("the promoted record publishes durably");
        assert_eq!(promoted_receipt.mechanism(), EXPECTED_DURABLE_MECHANISM_V1);
        assert_eq!(
            promoted_receipt.final_path(),
            std::fs::canonicalize(dir.join(&final_name)).expect("the promoted record exists")
        );

        // Create-new by construction: republishing over a live final name is
        // refused rather than silently replacing an immutable record.
        Cycle4BaselineChainAccessV1::publish_sidecar_bytes_v1(&dir, &final_name, &bytes)
            .expect_err("an immutable record is never replaced");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_stale_staging_file_never_blocks_a_replayed_publication_v1() {
        // An interrupted publication can leave the primitive's own staging
        // file behind. A replay removes it first, exactly as the chain's own
        // boundary publication does, so a retry cannot die on its own debris.
        let dir = fresh_temp_dir_v1("sidecar-stale-stage");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 11);
        let final_name = sidecar_record_name_v4(1).expect("name");
        let staged_name = cycle4_staged_sidecar_name_v1(&final_name);
        std::fs::create_dir_all(&dir).expect("create chain dir");
        std::fs::write(
            dir.join(cycle4_sidecar_stage_name_v1(&staged_name)),
            b"debris from an interrupted publication",
        )
        .expect("write debris");
        assert!(access.stage_sidecar_record_v4(1, &bytes));
        assert_eq!(access.sidecar_record_bytes_v4(1).expect("staged"), bytes);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn exactly_two_debris_grammars_are_removable_v1() {
        // Review finding P2: a process running the previous layout dies
        // before its rename and leaves `<name>.tmp-<pid>`; this layout's own
        // interrupted publication leaves `.<name>.stage-cycle4-v1`. Those two
        // shapes, and only those, are debris. Everything else in the shared
        // chain directory is either a staged record or none of our business,
        // and a name inside our own grammar that is malformed fails closed.
        let record = sidecar_record_name_v4(7).expect("name");
        let staged = cycle4_staged_sidecar_name_v1(&record);
        for (name, expected) in [
            // The two removable grammars, over both the record and the
            // staged name the previous and current layouts each produced.
            (
                format!("{record}.tmp-4436"),
                Some(Cycle4StagedEntryV1::Debris),
            ),
            (
                format!("{staged}.tmp-4436"),
                Some(Cycle4StagedEntryV1::Debris),
            ),
            (
                cycle4_sidecar_stage_name_v1(&staged),
                Some(Cycle4StagedEntryV1::Debris),
            ),
            (
                cycle4_sidecar_stage_name_v1(&record),
                Some(Cycle4StagedEntryV1::Debris),
            ),
            // The staged record itself.
            (staged.clone(), Some(Cycle4StagedEntryV1::Record(7))),
            // Another writer's names in the shared chain directory, and
            // another writer's debris: never ours to sweep.
            (record.clone(), Some(Cycle4StagedEntryV1::Foreign)),
            (
                "baseline-00000004.record.json".to_owned(),
                Some(Cycle4StagedEntryV1::Foreign),
            ),
            (
                "arm-origin.record.json".to_owned(),
                Some(Cycle4StagedEntryV1::Foreign),
            ),
            (
                "arm-origin.record.json.tmp-4436".to_owned(),
                Some(Cycle4StagedEntryV1::Foreign),
            ),
            (
                ".baseline-00000004.record.json.stage-v4".to_owned(),
                Some(Cycle4StagedEntryV1::Foreign),
            ),
            // A non-numeric process id is not the legacy grammar.
            (
                format!("{record}.tmp-notapid"),
                Some(Cycle4StagedEntryV1::Foreign),
            ),
            // In our grammar, but not a record name: the fail-closed case.
            (
                format!("baseline-update-0000000x.record.json{CYCLE4_STAGED_SIDECAR_SUFFIX_V1}"),
                None,
            ),
            (
                format!("not-a-sidecar{CYCLE4_STAGED_SIDECAR_SUFFIX_V1}"),
                None,
            ),
        ] {
            assert_eq!(
                classify_staged_entry_v1(&name),
                expected,
                "classifying {name}"
            );
        }
    }

    #[test]
    fn both_debris_shapes_are_swept_before_reconciliation_v1() {
        // Both grammars, in both namespaces, are deleted rather than
        // aborting the scan; a foreign name in the SHARED chain directory
        // survives untouched, and a foreign name in the legacy directory we
        // own outright still fails closed.
        let dir = fresh_temp_dir_v1("debris-sweep");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let legacy = dir.join(CYCLE4_LEGACY_STAGED_SIDECAR_DIRNAME_V1);
        std::fs::create_dir_all(&legacy).expect("create legacy dir");
        let record = sidecar_record_name_v4(3).expect("name");
        let staged = cycle4_staged_sidecar_name_v1(&record);

        // Debris the previous layout left in its own staging directory, and
        // debris this layout can leave in the chain directory.
        let legacy_temporary = legacy.join(format!("{record}.tmp-4436"));
        let chain_temporary = dir.join(format!("{staged}.tmp-4436"));
        let chain_stage = dir.join(cycle4_sidecar_stage_name_v1(&staged));
        let legacy_stage = legacy.join(cycle4_sidecar_stage_name_v1(&record));
        for path in [
            &legacy_temporary,
            &chain_temporary,
            &chain_stage,
            &legacy_stage,
        ] {
            std::fs::write(path, b"debris").expect("write debris");
        }
        // A neighbour in the shared directory that is none of our business.
        let foreign = dir.join("arm-origin.record.json");
        std::fs::write(&foreign, b"another writer's record").expect("write");

        access
            .sweep_staging_debris_v1()
            .expect("debris is removable");
        for path in [
            &legacy_temporary,
            &chain_temporary,
            &chain_stage,
            &legacy_stage,
        ] {
            assert!(!path.exists(), "{} must be swept", path.display());
        }
        assert!(foreign.is_file(), "another writer's file is never swept");

        // And the first reconciliation under this layout completes rather
        // than aborting on the legacy temporary, which is the regression.
        access
            .reconcile_staged_sidecars_v1(0)
            .expect("debris never aborts reconciliation");

        // A genuinely unexpected name in the directory we own outright still
        // fails closed.
        std::fs::write(legacy.join("unexpected.json"), b"?").expect("write");
        let error = access
            .staged_update_indexes_v1()
            .expect_err("an unknown name in our own staging directory fails closed");
        assert_eq!(error.code_v1(), "cycle4_arm_v1_baseline_sidecar_staging");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn records_staged_by_the_previous_layout_are_still_reconciled_v1() {
        // A Store left mid-segment by the previous layout has its records in
        // `baseline-staged/`. They are still found, still served to the walk,
        // still promoted, and the drained directory is then removed.
        let dir = fresh_temp_dir_v1("legacy-staged");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let legacy = dir.join(CYCLE4_LEGACY_STAGED_SIDECAR_DIRNAME_V1);
        std::fs::create_dir_all(&legacy).expect("create legacy dir");
        let mut state = NativeBaselineStateV4::empty_v4();
        let mut expected = Vec::new();
        for update_index in 1_u64..=4 {
            let bytes = synthetic_sidecar_v1(&state, update_index, update_index as u8);
            std::fs::write(
                legacy.join(sidecar_record_name_v4(update_index).expect("name")),
                &bytes,
            )
            .expect("write legacy staged record");
            state = access
                .replay_sidecar_v1(&state, update_index)
                .expect("a legacy staged record still replays");
            expected.push(bytes);
        }
        assert_eq!(
            access.staged_update_indexes_v1().expect("staged"),
            vec![1, 2, 3, 4]
        );
        access
            .reconcile_staged_sidecars_v1(4)
            .expect("all committed");
        assert!(access.commit_staged_sidecar_records_v4());
        for (offset, bytes) in expected.iter().enumerate() {
            let update_index = u64::try_from(offset).expect("offset") + 1;
            let promoted = dir.join(sidecar_record_name_v4(update_index).expect("name"));
            assert!(promoted.is_file(), "legacy staged records are promoted");
            assert_eq!(&std::fs::read(&promoted).expect("read"), bytes);
        }
        assert!(
            !legacy.exists(),
            "a drained legacy staging directory is removed"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Writes `bytes` as this update's staged record under BOTH layouts.
    fn stage_under_both_layouts_v1(
        access: &Cycle4BaselineChainAccessV1,
        dir: &Path,
        update_index: u64,
        current: &[u8],
        legacy: &[u8],
    ) {
        let record = sidecar_record_name_v4(update_index).expect("name");
        std::fs::create_dir_all(dir).expect("create chain dir");
        std::fs::write(dir.join(cycle4_staged_sidecar_name_v1(&record)), current)
            .expect("write current-layout staged record");
        let legacy_dir = access.legacy_staged_dir_v1();
        std::fs::create_dir_all(&legacy_dir).expect("create legacy dir");
        std::fs::write(legacy_dir.join(&record), legacy).expect("write legacy staged record");
    }

    #[test]
    fn the_same_update_staged_under_both_layouts_must_agree_v1() {
        // Review finding P2: deduplicating the two layouts by update index
        // hid a disagreement between them. The current copy was promoted and
        // the legacy copy left unread, so conflicting bytes could advance the
        // chain once before a later commit tripped over the leftover.
        let dir = fresh_temp_dir_v1("layout-conflict");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 13);
        let other = synthetic_sidecar_v1(&prior, 1, 14);
        assert_ne!(bytes, other);
        stage_under_both_layouts_v1(&access, &dir, 1, &bytes, &other);

        // Resolution names the conflict, and so does the reconcile that runs
        // before anything is promoted.
        let error = access
            .resolve_staged_record_v1(1)
            .expect_err("two layouts, two different records");
        assert_eq!(error.failure_v1(), Cycle4ArmFailureV1::Contract);
        assert_eq!(
            error.code_v1(),
            "cycle4_arm_v1_baseline_sidecar_layout_conflict"
        );
        let error = access
            .reconcile_staged_sidecars_v1(1)
            .expect_err("a conflict fails closed before promotion");
        assert_eq!(
            error.code_v1(),
            "cycle4_arm_v1_baseline_sidecar_layout_conflict"
        );
        // Nothing was promoted, and neither copy was consumed.
        assert!(!access.commit_staged_sidecar_records_v4());
        assert!(!dir.join(sidecar_record_name_v4(1).expect("name")).exists());
        assert!(access
            .legacy_staged_dir_v1()
            .join(sidecar_record_name_v4(1).expect("name"))
            .is_file());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_same_update_staged_identically_under_both_layouts_drains_both_v1() {
        // The agreeing branch: one promotion, and BOTH copies drain, so a
        // legacy leftover cannot outlive the promotion that consumed it.
        let dir = fresh_temp_dir_v1("layout-agree");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 13);
        stage_under_both_layouts_v1(&access, &dir, 1, &bytes, &bytes);

        let record = access
            .resolve_staged_record_v1(1)
            .expect("agreeing copies resolve")
            .expect("a staged record");
        assert_eq!(record.bytes, bytes);
        assert_eq!(record.copies.len(), 2, "both copies are tracked");
        assert_eq!(access.sidecar_record_bytes_v4(1).expect("staged"), bytes);
        assert_eq!(access.staged_update_indexes_v1().expect("staged"), vec![1]);

        access.reconcile_staged_sidecars_v1(1).expect("committed");
        assert!(access.commit_staged_sidecar_records_v4());
        let promoted = dir.join(sidecar_record_name_v4(1).expect("name"));
        assert!(promoted.is_file());
        assert_eq!(std::fs::read(&promoted).expect("read"), bytes);
        assert!(
            !access.legacy_staged_dir_v1().exists(),
            "the drained legacy copy and its directory are gone"
        );
        assert!(access
            .staged_update_indexes_v1()
            .expect("staged")
            .is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Creates a symlink at `link` pointing at `target`, or returns the
    /// reason it could not (unprivileged Windows hosts refuse).
    fn try_symlink_v1(target: &Path, link: &Path) -> std::result::Result<(), String> {
        #[cfg(windows)]
        let outcome = std::os::windows::fs::symlink_file(target, link);
        #[cfg(unix)]
        let outcome = std::os::unix::fs::symlink(target, link);
        outcome.map_err(|error| format!("{error}"))
    }

    #[test]
    fn a_reserved_name_that_is_not_a_regular_file_fails_closed_v1() {
        // Review finding P2: the scan skipped a non-regular entry while the
        // lookup followed it, so a resumed Store could validate and publish
        // its chain from bytes OUTSIDE the chain directory while the commit
        // reported success without ever creating the promoted sidecar.
        let dir = fresh_temp_dir_v1("irregular-staged");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 15);
        let outside = dir.join("outside-the-chain.json");
        std::fs::write(&outside, &bytes).expect("write the symlink target");
        let record = sidecar_record_name_v4(1).expect("name");
        let staged_name = dir.join(cycle4_staged_sidecar_name_v1(&record));

        let placeholder = match try_symlink_v1(&outside, &staged_name) {
            Ok(()) => "symlink",
            Err(reason) => {
                // Unprivileged Windows hosts refuse symlink creation; a
                // directory is the same class of non-regular entry and
                // exercises the same gate.
                eprintln!("symlink unavailable ({reason}); using a directory placeholder");
                std::fs::create_dir(&staged_name).expect("create directory placeholder");
                "directory"
            }
        };

        // The scan no longer skips it.
        let error = access
            .staged_update_indexes_v1()
            .expect_err("a reserved name that is not a regular file fails closed");
        assert_eq!(error.failure_v1(), Cycle4ArmFailureV1::Contract);
        assert_eq!(
            error.code_v1(),
            "cycle4_arm_v1_baseline_sidecar_irregular",
            "placeholder kind: {placeholder}"
        );
        // And the lookup no longer follows it, so the bytes on the other
        // side are never this update's record.
        assert!(
            access.sidecar_record_bytes_v4(1).is_none(),
            "a {placeholder} under a reserved name is never read through"
        );
        // Reconcile and commit both refuse rather than reporting success.
        assert_eq!(
            access
                .reconcile_staged_sidecars_v1(1)
                .expect_err("reconcile refuses")
                .code_v1(),
            "cycle4_arm_v1_baseline_sidecar_irregular"
        );
        assert!(!access.commit_staged_sidecar_records_v4());
        assert!(!dir.join(&record).exists(), "nothing was promoted");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_promoted_name_that_is_not_a_regular_file_is_never_trusted_v1() {
        // The same no-follow rule on the PROMOTED name: a symlink there must
        // not be read as the committed record, nor overwritten.
        let dir = fresh_temp_dir_v1("irregular-promoted");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 17);
        let outside = dir.join("outside-the-chain.json");
        std::fs::write(&outside, &bytes).expect("write the symlink target");
        let record = sidecar_record_name_v4(1).expect("name");
        let promoted = dir.join(&record);
        if let Err(reason) = try_symlink_v1(&outside, &promoted) {
            eprintln!("symlink unavailable ({reason}); using a directory placeholder");
            std::fs::create_dir(&promoted).expect("create directory placeholder");
        }
        assert!(
            access.sidecar_record_bytes_v4(1).is_none(),
            "a promoted name that is not a regular file is never read through"
        );
        // Staging refuses rather than treating the target's bytes as an
        // already-promoted record.
        assert!(!access.stage_sidecar_record_v4(1, &bytes));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_promotes_staged_records_for_committed_updates_v1() {
        // Reconcile case one: the Store committed these updates (its tip is
        // 4), so their staged sidecars survive the reconcile, are served to
        // the walk that revalidates them against that committed evidence,
        // and take their immutable names once the walk accepts the Store.
        let dir = fresh_temp_dir_v1("reconcile-promote");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        stage_segment_v1(&access, 4);
        access
            .reconcile_staged_sidecars_v1(4)
            .expect("every staged update is committed");
        assert_eq!(
            access.staged_update_indexes_v1().expect("staged"),
            vec![1, 2, 3, 4],
            "a committed update's staged record is kept for the walk"
        );
        assert!(access.commit_staged_sidecar_records_v4());
        for update_index in 1_u64..=4 {
            assert!(dir
                .join(sidecar_record_name_v4(update_index).expect("name"))
                .is_file());
        }
        assert!(access
            .staged_update_indexes_v1()
            .expect("staged")
            .is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reconcile_discards_staged_records_for_uncommitted_updates_v1() {
        // Reconcile case two: the producer stages before the Store publishes,
        // so a segment abandoned in that window leaves staged records for
        // updates the Store never committed. The next open discards exactly
        // those, and the retry stages the same updates cleanly.
        let dir = fresh_temp_dir_v1("reconcile-discard");
        {
            let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
            stage_segment_v1(&access, 4);
            // The segment is abandoned here: no Store publish, no promotion.
        }
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        assert_eq!(
            access.staged_update_indexes_v1().expect("staged"),
            vec![1, 2, 3, 4],
            "the staged records outlive the process that wrote them"
        );
        // The Store is still at generation 0, so none of them is committed.
        access
            .reconcile_staged_sidecars_v1(0)
            .expect("nothing committed");
        assert!(
            access
                .staged_update_indexes_v1()
                .expect("staged")
                .is_empty(),
            "a staged record for an uncommitted update is discarded"
        );
        assert!(access.sidecar_record_bytes_v4(1).is_none());
        assert_eq!(
            dir.read_dir()
                .expect("read dir")
                .filter_map(std::result::Result::ok)
                .count(),
            0,
            "nothing was promoted, and the discarded records are gone"
        );
        // A partially committed segment keeps exactly its committed prefix.
        let retry = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        stage_segment_v1(&retry, 4);
        retry
            .reconcile_staged_sidecars_v1(2)
            .expect("two updates committed");
        assert_eq!(
            retry.staged_update_indexes_v1().expect("staged"),
            vec![1, 2],
            "only the committed prefix survives"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_the_committed_tip_update_may_be_reconstructed_v1() {
        // Reconcile case three: a committed update with neither a staged nor
        // a promoted sidecar is re-derivable from its own committed evidence
        // ONLY at the Store's tip, where the producer's mint is exact. Every
        // earlier update whose sidecar is gone is unrecoverable and must
        // fail the walk closed.
        let dir = fresh_temp_dir_v1("reconcile-reconstruct");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        // Before any reconcile, nothing may be reconstructed.
        for update_index in 0_u64..=8 {
            assert!(!access.may_reconstruct_sidecar_v4(update_index));
        }
        access.reconcile_staged_sidecars_v1(4).expect("reconcile");
        assert!(access.may_reconstruct_sidecar_v4(4), "the tip update");
        for update_index in [0_u64, 1, 2, 3, 5, 8] {
            assert!(
                !access.may_reconstruct_sidecar_v4(update_index),
                "only the tip update is reconstructible, not {update_index}"
            );
        }
        // Generation 0 is the pre-training genesis and has no update at all.
        access.reconcile_staged_sidecars_v1(0).expect("reconcile");
        assert!(!access.may_reconstruct_sidecar_v4(0));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_sidecar_fails_the_boundary_replay_closed_v1() {
        let dir = fresh_temp_dir_v1("sidecar-missing");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        assert!(access.sidecar_record_bytes_v4(1).is_none());
        // Generation 0 is the empty pre-training state and needs no record.
        assert_eq!(
            access
                .committed_state_for_generation_v4(0)
                .expect("genesis state")
                .cell_count_v4(),
            0
        );
        // Generation 4 is one boundary ahead of an empty chain: it is only
        // reconstructible if every in-boundary sidecar is present.
        assert!(access.committed_state_for_generation_v4(4).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn in_boundary_sidecars_reconstruct_the_next_boundary_state_v1() {
        let dir = fresh_temp_dir_v1("sidecar-replay");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let mut expected = NativeBaselineStateV4::empty_v4();
        for update_index in 1_u64..=4 {
            let bytes = synthetic_sidecar_v1(&expected, update_index, update_index as u8);
            assert!(access.stage_sidecar_record_v4(update_index, &bytes));
            expected = access
                .replay_sidecar_v1(&expected, update_index)
                .expect("replay");
        }
        assert!(access.commit_staged_sidecar_records_v4());
        let resolved = access
            .committed_state_for_generation_v4(4)
            .expect("one boundary ahead of an empty chain is reconstructible");
        assert_eq!(resolved, expected);
        // Two boundaries ahead is never admitted, even with sidecars present.
        assert!(access.committed_state_for_generation_v4(8).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_tampered_sidecar_never_advances_the_replay_v1() {
        let dir = fresh_temp_dir_v1("sidecar-tampered");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 5);
        let decoded = decode_update_baseline_record_v4(&bytes).expect("decode");
        let cell = &decoded.cells()[0];
        let tampered = build_update_baseline_record_v4(UpdateBaselineRecordPartsV4 {
            update_index: 1,
            update_evidence_sha256: [5; 32],
            cells: vec![UpdateBaselineCellPartsV4 {
                opponent_checkpoint_manifest_sha256: cell
                    .key()
                    .opponent_checkpoint_manifest_sha256
                    .clone(),
                role: cell.key().role,
                c_t_bits: cell.c_t_bits(),
                c_next_bits: cell.c_next_bits() ^ 1,
                residual_sum_f64: cell.residual_sum_f64(),
                decision_count: cell.decision_count(),
                episode_count: cell.episode_count(),
            }],
            declared_policy_sum_bits: 0,
        })
        .expect("tampered record still encodes")
        .canonical_bytes()
        .to_vec();
        assert!(access.stage_sidecar_record_v4(1, &tampered));
        assert!(access.replay_sidecar_v1(&prior, 1).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_sidecar_namespace_never_collides_with_a_chain_boundary_record_v1() {
        let sidecar = sidecar_record_name_v4(3).expect("name");
        assert_eq!(sidecar, "baseline-update-00000003.record.json");
        assert_ne!(sidecar, "baseline-00000003.record.json");
    }
}
