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
//!   `stop_generation == resume_position + 128` before training.
//! - The origin binding (parent run, parent checkpoint/sidecar/state
//!   SHA-256s, init generation 896) lives in the hashed run record's
//!   `contracts.opponent_ladder_initialization`, and is additionally
//!   restated in this launcher's own hashed origin record published into the
//!   chain directory at genesis.

use crate::canonical_json_v1::{to_canonical_json_bytes_v1, CanonicalJsonNullPolicyV1};
use crate::native_baseline_checkpoint_chain_v4::{
    manifest_final_name_v4, publish_baseline_record_v4, record_final_name_v4,
    resume_baseline_chain_v4, sidecar_record_name_v4, BaselineChainRecordPartsV4,
    BaselineChainResumeVerdictV4,
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
    decode_cycle4_refresh_manifest_v1, Cycle4RefreshManifestV1, CYCLE4_REFRESH_INTERVAL_V1,
    CYCLE4_REFRESH_MAX_INDEX_V1, CYCLE4_SLOT_COUNT_V1, CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1,
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
use std::cell::RefCell;
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
    /// STORE generation this process stops at, `resume_position + 128`.
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
    let mut handles = Vec::with_capacity(POPULATION_OPPONENT_SLOT_COUNT_V1);
    for (slot, store_root) in manifest.slots_v1().iter().zip(slot_store_roots) {
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
                slot.source_generation,
                access,
            ),
            _ => load_native_training_boundary_v2(&root, &slot_run, slot.source_generation),
        }
        .map_err(|error| {
            mismatch(format!(
                "{} generation {}: {error}",
                store_root.display(),
                slot.source_generation
            ))
        })?;
        let checkpoint = boundary.checkpoint();
        let matches_authority = slot_run.run_sha256() == slot.source_run_sha256
            && slot_run.record().schedule.base_seed == slot.source_base_seed
            && checkpoint.generation_index() == slot.source_generation
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
struct Cycle4BaselineChainAccessV1 {
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
}

impl Cycle4BaselineChainAccessV1 {
    fn new_v1(chain_dir: PathBuf, checkpoint_segment_updates: u64) -> Self {
        Self {
            chain_dir,
            checkpoint_segment_updates,
            observed_core_hashes: RefCell::new(BTreeMap::new()),
            boundary_states: RefCell::new(BTreeMap::new()),
        }
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
        std::fs::read(self.sidecar_path_v1(update_index)?).ok()
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

    fn publish_sidecar_record_v4(&self, update_index: u64, record_bytes: &[u8]) -> bool {
        let Some(path) = self.sidecar_path_v1(update_index) else {
            return false;
        };
        if std::fs::create_dir_all(&self.chain_dir).is_err() {
            return false;
        }
        // Crash replay: an already-published sidecar with identical bytes is
        // the same publication, so accept it; different bytes are a genuine
        // conflict and fail closed.
        if let Ok(existing) = std::fs::read(&path) {
            return existing == record_bytes;
        }
        write_file_atomically_v1(&path, record_bytes).is_ok()
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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

/// The interval stop is a STORE generation: exactly one refresh interval past
/// the resume position, never past the program's own 2048-generation end.
///
/// `preflight_updates` is the bounded relaxation the CONTROL preflight ladder
/// needs and nothing else may use: `Some(n)` replaces the pre-registered 128
/// with `n`, still exact (`stop == resume + n`), still inside the program's
/// end, and still a whole number of checkpoint segments -- the Store advances
/// a segment at a time, so a window that is not a segment multiple could not
/// land on its own stop and would silently overshoot.
fn validate_interval_stop_v1(
    stop_generation: u64,
    resume_generation: u64,
    checkpoint_segment_updates: u64,
    contract: &Cycle4ArmContractV1,
    arm: Cycle4ArmKindV1,
    preflight_updates: Option<u64>,
) -> Result<()> {
    let window = match preflight_updates {
        None => CYCLE4_REFRESH_INTERVAL_V1,
        Some(updates) => {
            if updates == 0 || updates > CYCLE4_ARM_PREFLIGHT_MAX_UPDATES_V1 {
                return Err(Cycle4ArmErrorV1::contract(
                    "cycle4_arm_v1_preflight_updates_range",
                    format!(
                        "--preflight-updates must be 1..={CYCLE4_ARM_PREFLIGHT_MAX_UPDATES_V1}, got {updates}"
                    ),
                ));
            }
            updates
        }
    };
    let expected = resume_generation.checked_add(window).ok_or_else(|| {
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
    if checkpoint_segment_updates == 0 || !window.is_multiple_of(checkpoint_segment_updates) {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_interval_stop",
            "the training window must be a whole number of checkpoint segments",
        ));
    }
    // A preflight prefix runs one or more short windows inside the genesis
    // interval and never chains a manifest, so it is pinned to the genesis
    // manifest and bounded below the first refresh boundary rather than
    // matched to a manifest position it does not have.
    if preflight_updates.is_some() {
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
        return Ok(());
    }
    // A refresh-chained arm's manifest names the interval it opens; a
    // static-pool arm reuses the genesis manifest at every interval and
    // therefore binds no resume position of its own.
    if !arm.static_pool_v1() && contract.program_update != resume_generation {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_resume_position_mismatch",
            format!(
                "manifest refresh index {} opens store generation {}, but the store resumes at {resume_generation}",
                contract.refresh_index, contract.program_update
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
    claim_store_mode_marker_v1(&parent_dir, request.arm, &run, mode)?;
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
    let root = bootstrapped.into_root();

    // 4. Baseline chain: v4 arms only. CONTROL-R never installs one.
    let access = request.arm.uses_baseline_v4_v1().then(|| {
        Cycle4BaselineChainAccessV1::new_v1(
            request.chain_dir.clone(),
            run.checkpoint_segment_updates(),
        )
    });

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

/// Claims `parent_dir` (the Store prefix) for this run's mode, or fails
/// closed. Runs BEFORE the Store is bootstrapped so a wrong-mode invocation
/// never creates or touches a Store at all.
///
/// The arm and the run identity are fixed by whoever claimed the prefix
/// first. The mode admits exactly one transition, `bootstrap -> formal` or
/// `bootstrap -> preflight`; `formal` and `preflight` are terminal, so a
/// preflight is rejected on any prefix a formal run has trained and the
/// reverse holds too.
fn claim_store_mode_marker_v1(
    parent_dir: &Path,
    arm: Cycle4ArmKindV1,
    run: &ValidatedTrainRunV2,
    mode: Cycle4ArmStoreModeV1,
) -> Result<()> {
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
                return Ok(());
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
                return write_file_atomically_v1(&path, &bytes).map_err(|error| {
                    Cycle4ArmErrorV1::runtime("cycle4_arm_v1_mode_marker", error.to_string())
                });
            }
            Err(conflict())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(parent_dir).map_err(|error| {
                Cycle4ArmErrorV1::runtime("cycle4_arm_v1_mode_marker", error.to_string())
            })?;
            write_file_atomically_v1(&path, &bytes).map_err(|error| {
                Cycle4ArmErrorV1::runtime("cycle4_arm_v1_mode_marker", error.to_string())
            })
        }
        Err(error) => Err(Cycle4ArmErrorV1::runtime(
            "cycle4_arm_v1_mode_marker",
            format!("{}: {error}", path.display()),
        )),
    }
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
/// # Errors
///
/// Returns a classified [`Cycle4ArmErrorV1`]: `Contract` (bin exit code 3)
/// for any contract, locator, or already-seeded-Store rejection, `Runtime`
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
    if bootstrapped.latest_final_present() {
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_genesis_already_present",
            format!(
                "{} already holds a Store; --bootstrap-genesis only ever seeds a new one",
                request.store_root.display()
            ),
        ));
    }
    let root = bootstrapped.into_root();

    // 4. Publish genesis and the origin record.
    let genesis =
        author_genesis_from_parent_v1(&root, &run, &locator, &request.chain_dir, request.arm)?;

    // 5. The same final-store validation an interval exit performs.
    let access = request.arm.uses_baseline_v4_v1().then(|| {
        Cycle4BaselineChainAccessV1::new_v1(
            request.chain_dir.clone(),
            run.checkpoint_segment_updates(),
        )
    });
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
    let declared = run
        .record()
        .contracts()
        .opponent_ladder_initialization
        .as_ref()
        .ok_or_else(|| {
            Cycle4ArmErrorV1::contract(
                "cycle4_arm_v1_genesis_requires_origin",
                "a cycle-4 arm's genesis must be seeded from a pinned parent checkpoint",
            )
        })?;
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
    let identity = Cycle4ArmGenesisIdentityV1 {
        checkpoint_manifest_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256()),
        checkpoint_payload_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()),
        model_parameter_sha256: lower_hex_raw32_v1(checkpoint.model_parameter_sha256()),
        train_state_sha256: lower_hex_raw32_v1(checkpoint.train_state_sha256()),
    };
    publish_origin_record_v1(chain_dir, arm, run, declared, &identity)?;
    Ok(identity)
}

fn publish_origin_record_v1(
    chain_dir: &Path,
    arm: Cycle4ArmKindV1,
    run: &ValidatedTrainRunV2,
    declared: &crate::native_training_store_run_v2::OpponentLadderInitializationContractV1,
    identity: &Cycle4ArmGenesisIdentityV1,
) -> Result<()> {
    let record = Cycle4ArmOriginRecordV1 {
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
    let bytes = to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).map_err(
        |error| Cycle4ArmErrorV1::runtime("cycle4_arm_v1_origin_record", error.to_string()),
    )?;
    std::fs::create_dir_all(chain_dir).map_err(|error| {
        Cycle4ArmErrorV1::runtime("cycle4_arm_v1_origin_record", error.to_string())
    })?;
    let path = chain_dir.join(CYCLE4_ARM_ORIGIN_RECORD_FILENAME_V1);
    if let Ok(existing) = std::fs::read(&path) {
        if existing == bytes {
            return Ok(());
        }
        return Err(Cycle4ArmErrorV1::contract(
            "cycle4_arm_v1_origin_record_conflict",
            "an origin record with different bytes already exists",
        ));
    }
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
    // The full v4 walk both proves every persisted update's evidence against
    // the chain and hands back the Store's own core train-state hash per
    // boundary, which is the only admissible input to a chain manifest
    // decode.
    let state =
        validate_native_training_store_baseline_v4_v2(root, run, access).map_err(|error| {
            Cycle4ArmErrorV1::runtime("cycle4_arm_v1_validate_failed", error.to_string())
        })?;
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
        build_cycle4_refresh_manifest_v1, Cycle4RefreshSlotV1, FrozenOccupantIdentityCycle4V1,
        CYCLE4_ANCHOR_0_V1, CYCLE4_ANCHOR_1_V1, CYCLE4_CURRENT_0_V1,
        CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1, CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1,
        CYCLE4_EXPLOITER_0_V1, CYCLE4_EXPLOITER_1_V1, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1,
        CYCLE4_HISTORICAL_1_ROTATION_V1, CYCLE4_HISTORICAL_LAG_V1,
    };
    use crate::native_training_store_run_v2::test_fixture_bytes_population_program_v2_cycle4_v1;
    use crate::native_training_store_update_group_v4::{
        build_update_baseline_record_v4, UpdateBaselineCellPartsV4, UpdateBaselineRecordPartsV4,
    };

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
        for stop in [256_u64, 383, 385, 512] {
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
    fn store_root_parts_split_parent_and_basename_v1() {
        let (parent, basename) =
            store_root_parts_v1(Path::new("D:/cycle4/arm-a/store")).expect("split");
        assert_eq!(parent, PathBuf::from("D:/cycle4/arm-a"));
        assert_eq!(basename, "store");
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
    fn sidecar_publication_is_atomic_and_idempotent_v1() {
        let dir = fresh_temp_dir_v1("sidecar-publish");
        let access = Cycle4BaselineChainAccessV1::new_v1(dir.clone(), 4);
        let prior = NativeBaselineStateV4::empty_v4();
        let bytes = synthetic_sidecar_v1(&prior, 1, 3);
        assert!(access.publish_sidecar_record_v4(1, &bytes));
        let path = dir.join("baseline-update-00000001.record.json");
        assert!(path.is_file(), "sidecar must use the contract's exact name");
        assert_eq!(std::fs::read(&path).expect("read"), bytes);
        // Crash replay of the identical publication is accepted; different
        // bytes for the same update are not.
        assert!(access.publish_sidecar_record_v4(1, &bytes));
        assert!(!access.publish_sidecar_record_v4(1, b"different"));
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
            assert!(access.publish_sidecar_record_v4(update_index, &bytes));
            expected = access
                .replay_sidecar_v1(&expected, update_index)
                .expect("replay");
        }
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
        assert!(access.publish_sidecar_record_v4(1, &tampered));
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
