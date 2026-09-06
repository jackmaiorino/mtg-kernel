//! Cycle-5 arm launcher (round B).
//!
//! Contract: `docs/native_cycle5_arm_launcher_v1.md` sections 3 and 4. This
//! is the first-class, contract-validated entry point that replaces the
//! env-var `multirun_pilot_v1` test harness for the three cycle-5 arms. One
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
//!   `docs/native_population_refresh_manifest_cycle5_v1.md` states ("the
//!   arm's own store counts updates 0..=2048").
//! - Trainee-local numbering is `2048 + store_generation`, so the contract's
//!   start 2048 maps to store generation 0 and its stop 4096 maps to store
//!   generation 2048. The refresh manifest carries both: `program_update`
//!   IS the store generation and `trainee_local_generation` is
//!   `2048 + program_update`.
//! - `--stop-generation` and [`Cycle5ArmRequestV1::stop_generation`] are
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
//!   `source_generation - 2048` (see `store_generation_for_slot_v1`). A label
//!   below 2048, a translated generation the Store does not contain, and a
//!   loaded checkpoint whose identity hashes differ from the roster's all
//!   fail closed. Slots bound to OTHER runs are read at their labels
//!   verbatim, since those runs number their own stores.
//! - The origin binding (parent run, parent checkpoint/sidecar/state
//!   SHA-256s, init generation 2048) lives in the hashed run record's
//!   `contracts.opponent_ladder_initialization`, and is additionally
//!   restated in this launcher's own hashed origin record published into the
//!   chain directory at genesis.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonNullPolicyV1,
};
use crate::native_checkpoint_inference_v1::load_native_checkpoint_inference_v1;
use crate::native_ladder_pool_resolution_v1::{
    ladder_init_as_checkpoint_ref_v1, resolve_ladder_checkpoint_authority_v1,
    stage_ladder_checkpoint_initialization_v1,
};
use crate::native_policy_train_step_v1::NativeTrainingNumericalBackendV1;
use crate::native_population_opponent_v1::{
    PopulationOpponentEngineV1, PopulationSlotOccupantV1, PopulationWeightVectorV1,
    POPULATION_OPPONENT_SLOT_COUNT_V1,
};
use crate::native_population_refresh_builder_cycle5_v1::{
    cycle5_chain_manifest_filename_v1, cycle5_chain_panel_filename_v1,
};
use crate::native_population_refresh_manifest_cycle5_v1::{
    decode_cycle5_refresh_manifest_v1, Cycle5RefreshManifestV1, Cycle5RefreshSlotV1,
    CYCLE5_REFRESH_INTERVAL_V1, CYCLE5_REFRESH_MAX_INDEX_V1, CYCLE5_SLOT_COUNT_V1,
    CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1,
};
use crate::native_training_executor_v1::NativeTrainingExecutionConfigV1;
use crate::native_training_store_bootstrap_v2::bootstrap_native_training_store_v2;
use crate::native_training_store_boundary_v2::build_genesis_native_training_boundary_v2;
use crate::native_training_store_checkpoint_v3::{
    build_genesis_checkpoint_manifest_v2_v3, derive_genesis_weights_only_payload_v2_v3,
    CheckpointManifestV3,
};
use crate::native_training_store_digest_v1::lower_hex_raw32_v1;
use crate::native_training_store_layout_v2::NativeTrainingStoreFinalNameV2;
use crate::native_training_store_prepared_segment_v2::prepare_segment_v2;
use crate::native_training_store_reference_latest_v2::{
    build_checkpoint_reference_v2, build_latest_v2,
};
use crate::native_training_store_resume_v2::{
    load_native_training_boundary_v2, resume_native_training_store_with_session_v2,
    validate_native_training_store_v2, NativeTrainingStoreContinuationSessionV2,
    NativeTrainingStoreResumeV2,
};
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::{
    decode_train_run_v2, TrainerLossIdentityV2, ValidatedTrainRunV2,
};
use crate::native_training_store_segment_manifest_v2::build_genesis_segment_manifest_v2;
use crate::native_training_store_v2::{
    publish_genesis_generation_v2, publish_prepared_segment_with_session_v2,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Machine-local mapping from each pinned slot identity to the absolute
/// store root that holds it. Deliberately a SEPARATE schema from the payoff
/// panel runner's `mtg-kernel-cycle5-slot-locator/v1` (which is index-keyed):
/// this one is identity-keyed so a wrong store cannot occupy a right slot,
/// and the launcher cross-checks the whole key set against the manifest's
/// roster before any store is opened. Absolute paths never enter a hashed
/// contract, which is exactly why this file exists.
pub const CYCLE5_ARM_SLOT_LOCATOR_SCHEMA_V1: &str = "mtg-kernel-cycle5-arm-slot-locator/v1";

/// Launcher-level hashed record binding the arm's genesis origin: the parent
/// checkpoint the g896 state came from, the init generation, and the arm's
/// own run identity. Published once, atomically, into the chain directory
/// when the arm's Store genesis is authored.
pub const CYCLE5_ARM_ORIGIN_RECORD_SCHEMA_V1: &str = "mtg-kernel-cycle5-arm-origin/v1";

/// Fixed on-disk name of the origin record inside the chain directory.
pub const CYCLE5_ARM_ORIGIN_RECORD_FILENAME_V1: &str = "arm-origin.record.json";

/// Launcher-level marker pinning one Store prefix to one mode for the life of
/// that prefix. It lives in the Store root's PARENT directory (the "Store
/// prefix"), never inside the Store: the Store's own leaf grammar is closed
/// and a stray file under the root would be a layout violation.
///
/// The marker exists for one reason: `--preflight-updates` relaxes the
/// interval check from the pre-registered 128 to a short window, so a Store a
/// preflight ever TRAINED cannot become a formal artifact, and a formal Store
/// cannot be re-entered under the relaxed check. See
/// [`Cycle5ArmStoreModeV1`] for the one admissible transition.
pub const CYCLE5_ARM_MODE_MARKER_SCHEMA_V1: &str = "mtg-kernel-cycle5-arm-mode-marker/v1";

/// Fixed on-disk name of the mode marker inside the Store prefix.
pub const CYCLE5_ARM_MODE_MARKER_FILENAME_V1: &str = "cycle5-arm-mode.marker.json";

/// Largest `--preflight-updates` window. The preflight ladder only ever needs
/// a couple of updates per prefix; bounding it here keeps a mistyped flag from
/// quietly becoming a long relaxed-interval run.
pub const CYCLE5_ARM_PREFLIGHT_MAX_UPDATES_V1: u64 = 8;

/// Total Store generations the whole cycle-5 program runs (16 intervals of
/// 128), i.e. trainee-local 2048 through 4096.
const CYCLE5_ARM_STORE_GENERATION_TOTAL_V1: u64 =
    CYCLE5_REFRESH_MAX_INDEX_V1 * CYCLE5_REFRESH_INTERVAL_V1;

/// Which arm this process runs. The value must equal the arm the run record
/// itself declares; the launcher never infers one from the other.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cycle5ArmKindV1 {
    /// The frozen v3 recipe continued from the routing record's parent:
    /// refresh machinery on, no baseline chain, byte-for-byte the cycle-4
    /// CONTROL-R training path at a new start generation.
    ControlV3,
    /// The centered-baseline candidate. Declared so records, rosters and
    /// wrappers can name it; every launch path refuses it until the v5
    /// trainer contract is ratified and implemented.
    CenteredV5,
}

impl Cycle5ArmKindV1 {
    #[must_use]
    pub fn from_wire_v1(value: &str) -> Option<Self> {
        match value {
            "control-v3" => Some(Self::ControlV3),
            "centered-v5" => Some(Self::CenteredV5),
            _ => None,
        }
    }

    #[must_use]
    pub const fn wire_v1(self) -> &'static str {
        match self {
            Self::ControlV3 => "control-v3",
            Self::CenteredV5 => "centered-v5",
        }
    }

    /// Whether the arm trains under the (unratified) v5 centered baseline.
    /// `control-v3` never does and runs the frozen v3 path.
    #[must_use]
    pub const fn uses_centered_baseline_v1(self) -> bool {
        matches!(self, Self::CenteredV5)
    }

    /// The arm's formal TRAINING base seed.
    ///
    /// NOT RATIFIED. The owner has not yet ratified the cycle-5 seed bands
    /// (`OX_CYCLE5_PREREG_SKETCH_V1.md` open decision 2), so a production
    /// build carries the placeholder `CYCLE5_FORMAL_BASE_SEED_UNRATIFIED_V1`
    /// for every arm and `validate_run_contract_v1` refuses any record that
    /// declares it. The `cfg(test)` twin carries clearly fictitious values
    /// so the unit tests can exercise the seed gate. When the bands are
    /// ratified, replace the production literals here and nowhere else.
    #[must_use]
    pub const fn formal_base_seed_v1(self) -> u64 {
        #[cfg(not(test))]
        {
            match self {
                Self::ControlV3 | Self::CenteredV5 => CYCLE5_FORMAL_BASE_SEED_UNRATIFIED_V1,
            }
        }
        #[cfg(test)]
        {
            match self {
                Self::ControlV3 => 990_000,
                Self::CenteredV5 => 991_000,
            }
        }
    }

    /// Whether this build carries a ratified seed for the arm.
    #[must_use]
    pub const fn formal_base_seed_is_ratified_v1(self) -> bool {
        self.formal_base_seed_v1() != CYCLE5_FORMAL_BASE_SEED_UNRATIFIED_V1
    }

    /// No cycle-5 arm freezes its pool: both refresh.
    #[must_use]
    pub const fn static_pool_v1(self) -> bool {
        false
    }
}

/// The unratified base-seed placeholder. Zero is never a formal seed (every
/// earlier program's bands start above 900,000), so the gate cannot be
/// satisfied by accident.
pub const CYCLE5_FORMAL_BASE_SEED_UNRATIFIED_V1: u64 = 0;

/// One interval's complete, typed request. Every path is machine-local and
/// never enters a hashed artifact.
#[derive(Clone, Debug)]
pub struct Cycle5ArmRequestV1 {
    pub arm: Cycle5ArmKindV1,
    /// The Store root directory itself (its parent and basename are derived).
    pub store_root: PathBuf,
    /// The arm's formal `run.json` bytes on disk.
    pub run_record: PathBuf,
    /// The arm's baseline chain directory (boundary records, per-update
    /// sidecars, and the origin record).
    pub chain_dir: PathBuf,
    /// This interval's cycle-5 refresh manifest. Its own directory is the
    /// refresh chain directory: predecessors are read from it by the pinned
    /// `refresh-NN.manifest.json` / `refresh-NN.panel.json` naming scheme.
    pub refresh_manifest: PathBuf,
    /// The panel bytes the manifest binds by hash. Absent only for genesis
    /// (`refresh_index == 0`).
    pub payoff_panel: Option<PathBuf>,
    /// Identity-keyed slot locator, see
    /// [`CYCLE5_ARM_SLOT_LOCATOR_SCHEMA_V1`].
    pub slot_locator: PathBuf,
    /// STORE generation this process stops at: the end of the interval the
    /// manifest opens, a multiple of 128 at or below 2048. The Store may
    /// resume anywhere inside that interval (an interrupted attempt keeps its
    /// original stop), or at the stop itself when it already completed.
    pub stop_generation: u64,
    /// Bounded preflight provision (`docs/native_cycle5_arm_launcher_v1.md`
    /// Section 6's CONTROL preflight ladder). `None` is the formal path and
    /// is byte-for-byte the pre-registered behavior. `Some(n)` relaxes the
    /// interval check to `stop == resume + n` for `n` in
    /// `1 ..= CYCLE5_ARM_PREFLIGHT_MAX_UPDATES_V1`, and can only ever run
    /// against a throwaway Store prefix: the mode marker refuses a prefix
    /// that a formal run already claimed, and refuses to let a formal run
    /// re-enter a prefix a preflight claimed.
    pub preflight_updates: Option<u64>,
}

/// What one interval actually did.
#[derive(Clone, Debug)]
pub struct Cycle5ArmOutcomeV1 {
    pub arm: Cycle5ArmKindV1,
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
pub enum Cycle5ArmFailureV1 {
    Contract,
    Runtime,
}

#[derive(Clone, Debug)]
pub struct Cycle5ArmErrorV1 {
    failure: Cycle5ArmFailureV1,
    code: &'static str,
    detail: String,
}

impl Cycle5ArmErrorV1 {
    fn contract(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            failure: Cycle5ArmFailureV1::Contract,
            code,
            detail: detail.into(),
        }
    }

    fn runtime(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            failure: Cycle5ArmFailureV1::Runtime,
            code,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn failure_v1(&self) -> Cycle5ArmFailureV1 {
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
    /// (`docs/native_cycle5_arm_launcher_v1.md` Section 4).
    #[must_use]
    pub const fn exit_code_v1(&self) -> i32 {
        match self.failure {
            Cycle5ArmFailureV1::Contract => 3,
            Cycle5ArmFailureV1::Runtime => 1,
        }
    }
}

impl Display for Cycle5ArmErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for Cycle5ArmErrorV1 {}

type Result<T> = std::result::Result<T, Cycle5ArmErrorV1>;

// ---------------------------------------------------------------------
// Slot locator
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle5SlotLocatorEntryV1 {
    checkpoint_manifest_sha256: String,
    store_root: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle5SlotLocatorV1 {
    schema: String,
    stores: Vec<Cycle5SlotLocatorEntryV1>,
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

fn decode_slot_locator_v1(bytes: &[u8]) -> Result<Cycle5SlotLocatorV1> {
    let locator: Cycle5SlotLocatorV1 = serde_json::from_slice(bytes).map_err(|error| {
        Cycle5ArmErrorV1::contract("cycle5_arm_v1_slot_locator_malformed", error.to_string())
    })?;
    if locator.schema != CYCLE5_ARM_SLOT_LOCATOR_SCHEMA_V1 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_slot_locator_schema",
            locator.schema,
        ));
    }
    if locator.stores.len() != CYCLE5_SLOT_COUNT_V1 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_slot_locator_slot_count",
            format!("{} entries", locator.stores.len()),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &locator.stores {
        if !is_lower_hex_sha256_v1(&entry.checkpoint_manifest_sha256) {
            return Err(Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_slot_locator_identity",
                entry.checkpoint_manifest_sha256.clone(),
            ));
        }
        if !seen.insert(entry.checkpoint_manifest_sha256.clone()) {
            return Err(Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_slot_locator_duplicate_identity",
                entry.checkpoint_manifest_sha256.clone(),
            ));
        }
        if entry.store_root.is_empty() || !Path::new(&entry.store_root).is_absolute() {
            return Err(Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_slot_locator_relative_path",
                entry.store_root.clone(),
            ));
        }
    }
    if let Some(parent) = &locator.genesis_parent_store_root {
        if parent.is_empty() || !Path::new(parent).is_absolute() {
            return Err(Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_slot_locator_relative_path",
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
    locator: &Cycle5SlotLocatorV1,
    manifest: &Cycle5RefreshManifestV1,
) -> Result<Vec<PathBuf>> {
    let mut by_identity: BTreeMap<&str, &str> = BTreeMap::new();
    for entry in &locator.stores {
        by_identity.insert(
            entry.checkpoint_manifest_sha256.as_str(),
            entry.store_root.as_str(),
        );
    }
    let slots = manifest.slots_v1();
    if slots.len() != CYCLE5_SLOT_COUNT_V1 || by_identity.len() != CYCLE5_SLOT_COUNT_V1 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_slot_locator_slot_count",
            "roster and locator must both carry exactly eight slots",
        ));
    }
    let mut roots = Vec::with_capacity(CYCLE5_SLOT_COUNT_V1);
    for slot in slots {
        let root = by_identity
            .remove(slot.checkpoint_manifest_sha256.as_str())
            .ok_or_else(|| {
                Cycle5ArmErrorV1::contract(
                    "cycle5_arm_v1_slot_locator_roster_mismatch",
                    format!(
                        "slot {} identity {} is not in the locator",
                        slot.slot_index, slot.checkpoint_manifest_sha256
                    ),
                )
            })?;
        roots.push(PathBuf::from(root));
    }
    if !by_identity.is_empty() {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_slot_locator_roster_mismatch",
            "locator carries an identity the roster does not",
        ));
    }
    Ok(roots)
}

// ---------------------------------------------------------------------
// Read-only slot-locator decode check (`--check-slot-locator`)
// ---------------------------------------------------------------------

/// What one `--check-slot-locator` pass proved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cycle5SlotLocatorCheckOutcomeV1 {
    /// How many `run.json` records decoded: the locator's eight slot Stores,
    /// plus the genesis parent when the locator names one.
    pub decoded_run_record_count: usize,
    /// Whether the locator carried a `genesis_parent_store_root` and it was
    /// therefore decoded too.
    pub genesis_parent_checked: bool,
}

/// Decodes every `run.json` a slot locator points at, and the genesis
/// parent's, and nothing else.
///
/// Cycle-5 round F, item 3. The CONTROL preflight ladder's first attempt
/// spent two full five-minute genesis bootstraps before either arm rung
/// reached the point of resolving its opponent slots and refused there, on
/// a record that could not decode. Every input that refusal depended on was
/// readable before the first bootstrap started. This mode reads them, in
/// exactly the way the slot resolver later will
/// (`decode_train_run_v2`, the same entry point
/// `resolve_population_opponent_cycle5_v1` calls, and the same one
/// `resolve_ladder_checkpoint_authority_v1` calls for the parent), and says
/// yes or no in under a second.
///
/// Strictly read-only and device-free: it opens no Store root, reads no
/// checkpoint, claims no Store-prefix mode marker, allocates no CUDA
/// context, and writes nothing anywhere. The only files it touches are the
/// locator and one `run.json` per named Store, all opened for reading.
///
/// Deliberately NOT the full slot resolution: a locator is checked on its
/// own, without a refresh manifest, so this proves decodability, not
/// identity binding. Identity binding needs a manifest and is still proven
/// where it always was, at the slot resolver.
///
/// # Errors
///
/// Returns a classified [`Cycle5ArmErrorV1`]. Every rejection this mode can
/// produce is `Contract` (bin exit code 3), including an unreadable locator
/// or `run.json`: from a launcher's point of view "the inputs you named
/// cannot be read" and "the inputs you named cannot be decoded" are the same
/// answer, and a check mode with exactly two outcomes is easier to wire into
/// a wrapper than one with three.
pub fn run_native_cycle5_arm_check_slot_locator_v1(
    slot_locator: &Path,
) -> Result<Cycle5SlotLocatorCheckOutcomeV1> {
    let locator_bytes = std::fs::read(slot_locator).map_err(|error| {
        Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_slot_locator_read",
            format!("{}: {error}", slot_locator.display()),
        )
    })?;
    let locator = decode_slot_locator_v1(&locator_bytes)?;

    let mut decoded_run_record_count = 0_usize;
    let mut check_one = |store_root: &str| -> Result<()> {
        let run_json = Path::new(store_root).join("run.json");
        let bytes = std::fs::read(&run_json).map_err(|error| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_slot_locator_store_run_read",
                format!("{}: {error}", run_json.display()),
            )
        })?;
        decode_train_run_v2(&bytes).map_err(|error| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_slot_locator_store_run_rejected",
                format!("{}: {error}", run_json.display()),
            )
        })?;
        decoded_run_record_count += 1;
        Ok(())
    };

    for entry in &locator.stores {
        check_one(&entry.store_root)?;
    }
    let genesis_parent_checked = match &locator.genesis_parent_store_root {
        Some(parent) => {
            check_one(parent)?;
            true
        }
        None => false,
    };

    Ok(Cycle5SlotLocatorCheckOutcomeV1 {
        decoded_run_record_count,
        genesis_parent_checked,
    })
}

// ---------------------------------------------------------------------
// Cycle-5 population resolution (sibling of resolve_population_opponent_v1)
// ---------------------------------------------------------------------

/// The Store generation one roster slot is actually read at.
///
/// The refresh manifest labels every slot in the contract's trainee-local
/// numbering (`docs/native_population_refresh_manifest_cycle5_v1.md`,
/// Frame). For a slot bound to the ARM'S OWN run that label is 2048 above the
/// arm's Store numbering, because the arm is a new run identity seeded from
/// the cycle-3 g2048 checkpoint and its Store publishes genesis at generation
/// 0 (`0 ..= 2048` for `2048 ..= 4096`). Translation lives here rather than
/// in the manifest so every identity in the roster keeps one consistent
/// lineage numbering.
///
/// Fails closed on an own-run label below 2048: there is no Store generation
/// such a label could name. Slots bound to other runs are returned
/// unchanged; those runs number their own Stores.
fn store_generation_for_slot_v1(slot: &Cycle5RefreshSlotV1, arm_run_sha256: &str) -> Result<u64> {
    if slot.source_run_sha256 != arm_run_sha256 {
        return Ok(slot.source_generation);
    }
    slot.source_generation
        .checked_sub(CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1)
        .ok_or_else(|| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_own_run_slot_generation",
                format!(
                    "slot {} names the arm's own run at trainee-local generation {}, which is below the program start {CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1}",
                    slot.slot_index, slot.source_generation
                ),
            )
        })
}

/// Cycle-5 sibling of `native_population_runtime_resolution_v1`'s
/// `resolve_population_opponent_v1`: same shape (reopen each slot's own Store
/// through its own `run.json` and complete walk, re-verify every declared
/// identity hash against the actually loaded artifact, then build the eight
/// immutable inference handles), retyped onto the cycle-5 slot record's
/// five-hash identity. Cycle-5 admits no search occupants, so every slot is
/// Store-backed.
/// From refresh index 1 the roster's `current-1` slot (and from index 4 its
/// `historical-0` slot) binds the ARM'S OWN run. The control arm's own
/// Store is a v3 Store, so every slot resolves through the plain walk; a
/// centered-baseline Store would need its own walk, which this build does
/// not carry (the arm is refused before resolution).
///
/// Own-run slots additionally translate their trainee-local
/// `source_generation` label into the arm's Store numbering
/// (`store_generation_for_slot_v1`) before the boundary is opened, and the
/// loaded checkpoint's `generation_index` is compared against the TRANSLATED
/// value; other-run slots keep their labels verbatim.
fn resolve_population_opponent_cycle5_v1(
    manifest: &Cycle5RefreshManifestV1,
    slot_store_roots: &[PathBuf],
    arm_run_sha256: &str,
) -> Result<PopulationOpponentEngineV1> {
    if slot_store_roots.len() != POPULATION_OPPONENT_SLOT_COUNT_V1
        || manifest.slots_v1().len() != POPULATION_OPPONENT_SLOT_COUNT_V1
    {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_population_slot_count",
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
            Cycle5ArmErrorV1::contract("cycle5_arm_v1_population_authority_mismatch", detail)
        };
        let run_bytes = std::fs::read(store_root.join("run.json")).map_err(|error| {
            Cycle5ArmErrorV1::runtime(
                "cycle5_arm_v1_population_run_read",
                format!("{}: {error}", store_root.display()),
            )
        })?;
        let slot_run = decode_train_run_v2(&run_bytes)
            .map_err(|error| mismatch(format!("{} run.json: {error}", store_root.display())))?;
        let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root).map_err(|error| {
            Cycle5ArmErrorV1::runtime(
                "cycle5_arm_v1_population_root_open",
                format!("{}: {error}", store_root.display()),
            )
        })?;
        let boundary = load_native_training_boundary_v2(&root, &slot_run, store_generation)
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
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_population_slot_count",
                "eight handles required",
            )
        })?;
    let weight_units: [u64; POPULATION_OPPONENT_SLOT_COUNT_V1] =
        std::array::from_fn(|index| manifest.slots_v1()[index].weight_units);
    let total = weight_units
        .iter()
        .try_fold(0_u64, |sum, weight| sum.checked_add(*weight))
        .ok_or_else(|| {
            Cycle5ArmErrorV1::contract("cycle5_arm_v1_population_weight", "weight total overflow")
        })?;
    let weights = PopulationWeightVectorV1::new_v1(weight_units, total).map_err(|error| {
        Cycle5ArmErrorV1::contract("cycle5_arm_v1_population_weight", error.to_string())
    })?;
    Ok(PopulationOpponentEngineV1::new_v1(weights, handles))
}

// ---------------------------------------------------------------------
// Head-to-head boundary mode (payoff panel probe)
// ---------------------------------------------------------------------

/// How one side of a head-to-head evaluation loads its checkpoint boundary.
/// Cycle 5 has exactly one admissible mode until the v5 trainer exists.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cycle5H2hBoundaryModeV1 {
    /// `load_native_training_boundary_v2`: the frozen v3 walk.
    Plain,
}

/// Decides how one head-to-head side must load its boundary. A side whose
/// run declares the v5 trainer cannot be loaded by this build at all (the
/// walk for it does not exist yet), and a chain directory handed to a v3 run
/// would be silently ignored, so both are refused rather than accepted.
#[cfg(test)]
pub(crate) fn cycle5_h2h_boundary_mode_v1(
    declares_trainer_v5: bool,
    chain_dir: Option<&str>,
) -> std::result::Result<Cycle5H2hBoundaryModeV1, &'static str> {
    match (declares_trainer_v5, chain_dir) {
        (false, None) => Ok(Cycle5H2hBoundaryModeV1::Plain),
        (true, _) => Err("cycle5_h2h_v5_run_unratified"),
        (false, Some(_)) => Err("cycle5_h2h_chain_dir_requires_v5_run"),
    }
}

// ---------------------------------------------------------------------
// Origin record
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle5ArmOriginRecordV1 {
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
/// cycle-5 slot roster wants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cycle5ArmGenesisIdentityV1 {
    pub checkpoint_manifest_sha256: String,
    pub checkpoint_payload_sha256: String,
    pub model_parameter_sha256: String,
    pub train_state_sha256: String,
}

// ---------------------------------------------------------------------
// Contract validation
// ---------------------------------------------------------------------

#[derive(Debug)]
struct Cycle5ArmContractV1 {
    refresh_index: u64,
    /// The Store generation this manifest opens: `refresh_index * 128`.
    program_update: u64,
    /// `2048 + program_update`, the same number in the contract's
    /// trainee-local numbering. Kept so the mapping is proven, not assumed.
    #[allow(dead_code)]
    trainee_local_generation: u64,
}

/// Whether a Store root already carries a published `latest` pointer, read
/// without creating or touching anything.
///
/// The bootstrap engine answers the same question authoritatively, but only
/// as a side effect of creating the root, so it cannot be used to decide
/// whether an invocation should proceed at all. This looks at the one final
/// name that answers it.
fn store_holds_published_genesis_v1(store_root: &Path) -> Result<bool> {
    let leaf = NativeTrainingStoreFinalNameV2::Latest
        .final_basename()
        .map_err(|error| {
            Cycle5ArmErrorV1::runtime("cycle5_arm_v1_store_layout", error.to_string())
        })?;
    Ok(store_root.join(leaf).is_file())
}

// ---------------------------------------------------------------------
// Build provenance (round-E review round 3)
//
// A run record declares the build that publishes the Store. Nothing made
// the ARM prove that claim was about ITSELF: a record built by one build's
// cycle5_run_record_v1 and an arm binary from another build produced a
// Store whose record attributes it to the wrong source tree, and every
// validator passed. The arm now captures its own embedded build metadata
// and its own executable at every launch and requires exact equality.
// ---------------------------------------------------------------------

#[cfg(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
))]
fn require_run_record_is_this_build_v1(run: &ValidatedTrainRunV2) -> Result<()> {
    // Fully-qualified rather than imported at module scope: these two are
    // used only here, and a module-scope import would be unused in a build
    // without production capture.
    crate::native_store_production_capture_v2::require_run_record_matches_current_launcher_build_v2(
        run,
        crate::native_training_store_run_v2::CYCLE5_ARM_LAUNCHER_BINARY_NAME_V1,
        crate::native_training_store_run_v2::CUDA_RUNTIME_TUPLE_IDENTITY_V2,
        crate::native_policy_train_step_v1::CUDA_BURN_DENSE_NUMERICAL_BACKEND_IDENTITY_V1,
    )
    .map_err(|error| {
        Cycle5ArmErrorV1::contract("cycle5_arm_v1_build_provenance_mismatch", error.to_string())
    })
}

#[cfg(not(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
)))]
fn require_run_record_is_this_build_v1(_run: &ValidatedTrainRunV2) -> Result<()> {
    // Fail closed rather than skip: a build that cannot capture production
    // provenance cannot prove a run record describes it, and an arm that
    // cannot prove that must not publish a Store.
    Err(Cycle5ArmErrorV1::contract(
        "cycle5_arm_v1_build_provenance_mismatch",
        "this build cannot capture production provenance, so it cannot prove the run record describes it",
    ))
}

/// This binary's own embedded build identity as canonical JSON.
///
/// `cycle5_arm_v1 --print-build-identity` writes exactly these bytes, and
/// `cycle5_run_record_v1` compares them against its own before it will build
/// a record naming this launcher: two binaries from different builds must not
/// be able to co-author one Store's provenance.
///
/// # Errors
///
/// Returns a classified [`Cycle5ArmErrorV1`] if this build cannot report an
/// identity at all.
#[cfg(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
))]
pub fn cycle5_arm_build_identity_json_v1() -> Result<String> {
    crate::native_store_production_capture_v2::current_launcher_build_identity_json_v2().map_err(
        |error| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_build_identity_unavailable",
                error.to_string(),
            )
        },
    )
}

/// See the production sibling above.
///
/// # Errors
///
/// Always: a build without production capture has no identity to report.
#[cfg(not(all(
    feature = "native-training-store-v2-production",
    target_os = "windows",
    target_env = "msvc",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    not(debug_assertions)
)))]
pub fn cycle5_arm_build_identity_json_v1() -> Result<String> {
    Err(Cycle5ArmErrorV1::contract(
        "cycle5_arm_v1_build_identity_unavailable",
        "this build embeds no production build-capture tuple",
    ))
}

/// The record-level cycle-5 arm check, exposed for the run-record BUILDER
/// (`native_cycle5_run_record_v1`) so a record is proven acceptable to this
/// launcher before it is ever written, rather than only when the first
/// invocation reads it back. Exactly [`validate_run_contract_v1`], no
/// separate restatement: what the builder proves is what the launcher
/// enforces, by construction.
///
/// # Errors
///
/// Returns the same classified [`Cycle5ArmErrorV1`] an invocation would.
pub fn validate_cycle5_arm_run_record_v1(
    run: &ValidatedTrainRunV2,
    arm: Cycle5ArmKindV1,
) -> Result<()> {
    validate_run_contract_v1(run, arm)?;
    validate_device_contract_v1(run, arm)?;
    Ok(())
}

fn validate_run_contract_v1(run: &ValidatedTrainRunV2, arm: Cycle5ArmKindV1) -> Result<()> {
    let contracts = run.record().contracts();
    let program = contracts
        .population_program_v2_cycle5
        .as_ref()
        .ok_or_else(|| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_missing_population_program_v2_cycle5",
                "the run record declares no population_program_v2_cycle5 section",
            )
        })?;
    if program.arm_kind != arm.wire_v1() {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_arm_kind_mismatch",
            format!(
                "requested {} but the run declares {}",
                arm.wire_v1(),
                program.arm_kind
            ),
        ));
    }
    if program.static_pool != arm.static_pool_v1() {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_arm_kind_mismatch",
            "static_pool disagrees with the arm kind",
        ));
    }
    if program.refresh_interval != CYCLE5_REFRESH_INTERVAL_V1 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_refresh_interval",
            program.refresh_interval.to_string(),
        ));
    }
    // Arm-kind consistency, restated here rather than trusted from decode:
    // centered-v5 requires the v5 trainer section, control-v3 forbids every
    // candidate trainer section and keeps the frozen v3 loss.
    if contracts.trainer_v4_candidate.is_some() {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_trainer_section_mismatch",
            "a cycle-5 arm never carries trainer_v4_candidate",
        ));
    }
    let declares_v5 = contracts.trainer_v5_candidate.is_some();
    if declares_v5 != arm.uses_centered_baseline_v1() {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_trainer_section_mismatch",
            format!(
                "{} {} trainer_v5_candidate",
                arm.wire_v1(),
                if arm.uses_centered_baseline_v1() {
                    "requires"
                } else {
                    "forbids"
                }
            ),
        ));
    }
    if arm.uses_centered_baseline_v1() {
        // Declared, not implemented: the v5 contract is unratified and this
        // build carries no v5 training path. Fail closed before any Store is
        // opened.
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_centered_v5_unratified",
            "the centered-v5 arm cannot launch until the v5 trainer contract is ratified and implemented",
        ));
    }
    if !matches!(
        contracts.trainer_loss_identity_v2(),
        TrainerLossIdentityV2::V3
    ) {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_trainer_section_mismatch",
            "control-v3 requires the frozen v3 loss identity",
        ));
    }
    if !arm.formal_base_seed_is_ratified_v1() {
        // The seed bands are not ratified: this build cannot train formally.
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_base_seed_unratified",
            format!(
                "{} carries the unratified base-seed placeholder; ratify the cycle-5 seed bands and replace the literal",
                arm.wire_v1()
            ),
        ));
    }
    // The arm-to-base-seed mapping is a pre-registered fact about the ARM,
    // not a property of whoever wrote the record, so it is enforced here
    // rather than only where the record is built: an operator-supplied
    // record, or one built for a different arm, cannot put one arm's seed
    // under another arm's kind.
    let expected_base_seed = arm.formal_base_seed_v1();
    let declared_base_seed = run.record().schedule().base_seed;
    if declared_base_seed != expected_base_seed {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_base_seed_mismatch",
            format!(
                "{} trains under base seed {expected_base_seed}, but the run record declares {declared_base_seed}",
                arm.wire_v1()
            ),
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
    arm: Cycle5ArmKindV1,
) -> Result<NativeTrainingNumericalBackendV1> {
    let backend = run.store_numerical_backend_v2().ok_or_else(|| {
        Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_device_contract",
            "the run record binds no Store numerical backend",
        )
    })?;
    if arm.uses_centered_baseline_v1() && backend != NativeTrainingNumericalBackendV1::CudaBurnDense {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_device_contract",
            "the centered-v5 arm admits only the CudaBurnDense numerical backend",
        ));
    }
    Ok(backend)
}

fn execution_config_from_run_v1(
    run: &ValidatedTrainRunV2,
    backend: NativeTrainingNumericalBackendV1,
) -> Result<NativeTrainingExecutionConfigV1> {
    let invalid = |detail: &str| {
        Cycle5ArmErrorV1::contract("cycle5_arm_v1_execution_config", detail.to_owned())
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
/// manifests bind their predecessor by hash, and the cycle-5 decoder has no
/// format-only acceptance path, so the whole chain is re-derived from the
/// manifest file's own directory using the pinned
/// `refresh-NN.manifest.json` / `refresh-NN.panel.json` naming scheme (the
/// same scheme `cycle5_refresh_build_v1` writes).
fn decode_interval_manifest_v1(
    manifest_path: &Path,
    panel_path: Option<&Path>,
) -> Result<Cycle5RefreshManifestV1> {
    let chain_dir = manifest_path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let manifest_bytes = std::fs::read(manifest_path).map_err(|error| {
        Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_refresh_manifest_read",
            format!("{}: {error}", manifest_path.display()),
        )
    })?;
    let panel_bytes = panel_path
        .map(|path| {
            std::fs::read(path).map_err(|error| {
                Cycle5ArmErrorV1::runtime(
                    "cycle5_arm_v1_payoff_panel_read",
                    format!("{}: {error}", path.display()),
                )
            })
        })
        .transpose()?;

    // Genesis first: it is the only manifest that decodes without a
    // predecessor, and it must carry no panel.
    let genesis_path = chain_dir.join(cycle5_chain_manifest_filename_v1(0));
    let genesis_bytes = std::fs::read(&genesis_path).map_err(|error| {
        Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_refresh_chain_read",
            format!("{}: {error}", genesis_path.display()),
        )
    })?;
    let mut current =
        decode_cycle5_refresh_manifest_v1(&genesis_bytes, None, None).map_err(|error| {
            Cycle5ArmErrorV1::contract("cycle5_arm_v1_refresh_manifest_rejected", error.to_string())
        })?;
    if genesis_bytes == manifest_bytes {
        if panel_bytes.is_some() {
            return Err(Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_genesis_takes_no_panel",
                "the genesis refresh manifest binds no payoff panel",
            ));
        }
        return Ok(current);
    }
    let panel_bytes = panel_bytes.ok_or_else(|| {
        Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_missing_payoff_panel",
            "a non-genesis refresh manifest requires its panel bytes",
        )
    })?;
    for refresh_index in 1..=CYCLE5_REFRESH_MAX_INDEX_V1 {
        let link_path = chain_dir.join(cycle5_chain_manifest_filename_v1(refresh_index));
        let link_bytes = std::fs::read(&link_path).map_err(|error| {
            Cycle5ArmErrorV1::runtime(
                "cycle5_arm_v1_refresh_chain_read",
                format!("{}: {error}", link_path.display()),
            )
        })?;
        let is_target = link_bytes == manifest_bytes;
        let link_panel = if is_target {
            panel_bytes.clone()
        } else {
            let panel_path = chain_dir.join(cycle5_chain_panel_filename_v1(refresh_index));
            std::fs::read(&panel_path).map_err(|error| {
                Cycle5ArmErrorV1::runtime(
                    "cycle5_arm_v1_refresh_chain_read",
                    format!("{}: {error}", panel_path.display()),
                )
            })?
        };
        current = decode_cycle5_refresh_manifest_v1(
            &link_bytes,
            Some(&current),
            Some(link_panel.as_slice()),
        )
        .map_err(|error| {
            Cycle5ArmErrorV1::contract("cycle5_arm_v1_refresh_manifest_rejected", error.to_string())
        })?;
        if is_target {
            return Ok(current);
        }
    }
    Err(Cycle5ArmErrorV1::contract(
        "cycle5_arm_v1_refresh_manifest_not_in_chain",
        format!(
            "{} is not a link of the chain rooted at {}",
            manifest_path.display(),
            genesis_path.display()
        ),
    ))
}

fn validate_manifest_against_run_v1(
    manifest: &Cycle5RefreshManifestV1,
    run: &ValidatedTrainRunV2,
    arm: Cycle5ArmKindV1,
) -> Result<Cycle5ArmContractV1> {
    if manifest.trainee_run_sha256_v1() != run.run_sha256()
        || manifest.trainee_base_seed_v1() != run.record().schedule().base_seed
    {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_manifest_run_binding",
            "the manifest's trainee identity is not this run",
        ));
    }
    if arm.static_pool_v1() && manifest.refresh_index_v1() != 0 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_static_pool_manifest_advanced",
            format!(
                "static-rb never advances past the genesis manifest, got refresh index {}",
                manifest.refresh_index_v1()
            ),
        ));
    }
    let program_update = manifest
        .refresh_index_v1()
        .checked_mul(CYCLE5_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_manifest_generation",
                "program update overflow",
            )
        })?;
    let trainee_local_generation = CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1
        .checked_add(program_update)
        .ok_or_else(|| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_manifest_generation",
                "trainee-local generation overflow",
            )
        })?;
    if manifest.trainee_local_generation_v1() != trainee_local_generation {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_manifest_generation",
            "the manifest's trainee-local generation is not 2048 plus its program update",
        ));
    }
    Ok(Cycle5ArmContractV1 {
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
/// `1 ..= CYCLE5_ARM_PREFLIGHT_MAX_UPDATES_V1`, still inside the program's
/// end, still a whole number of checkpoint segments, and still pinned to the
/// genesis manifest below the first refresh boundary. A preflight prefix is
/// throwaway, so it has no interrupted-attempt case to serve.
fn validate_interval_stop_v1(
    stop_generation: u64,
    resume_generation: u64,
    checkpoint_segment_updates: u64,
    contract: &Cycle5ArmContractV1,
    arm: Cycle5ArmKindV1,
    preflight_updates: Option<u64>,
) -> Result<()> {
    let interval_stop =
        |detail: String| Cycle5ArmErrorV1::contract("cycle5_arm_v1_interval_stop", detail);
    if let Some(updates) = preflight_updates {
        return validate_preflight_stop_v1(
            stop_generation,
            resume_generation,
            checkpoint_segment_updates,
            contract,
            updates,
        );
    }
    if stop_generation > CYCLE5_ARM_STORE_GENERATION_TOTAL_V1 {
        return Err(interval_stop(format!(
            "stop generation {stop_generation} is past the program end {CYCLE5_ARM_STORE_GENERATION_TOTAL_V1}"
        )));
    }
    if stop_generation == 0 || !stop_generation.is_multiple_of(CYCLE5_REFRESH_INTERVAL_V1) {
        return Err(interval_stop(format!(
            "stop generation {stop_generation} is not a whole refresh interval"
        )));
    }
    let interval_start = stop_generation
        .checked_sub(CYCLE5_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            interval_stop(format!(
                "stop generation {stop_generation} is not one refresh interval past any start"
            ))
        })?;
    if checkpoint_segment_updates == 0
        || !CYCLE5_REFRESH_INTERVAL_V1.is_multiple_of(checkpoint_segment_updates)
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
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_resume_position_mismatch",
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
    contract: &Cycle5ArmContractV1,
    updates: u64,
) -> Result<()> {
    if updates == 0 || updates > CYCLE5_ARM_PREFLIGHT_MAX_UPDATES_V1 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_preflight_updates_range",
            format!(
                "--preflight-updates must be 1..={CYCLE5_ARM_PREFLIGHT_MAX_UPDATES_V1}, got {updates}"
            ),
        ));
    }
    let expected = resume_generation.checked_add(updates).ok_or_else(|| {
        Cycle5ArmErrorV1::contract("cycle5_arm_v1_interval_stop", "stop generation overflow")
    })?;
    if stop_generation != expected {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_interval_stop",
            format!("expected stop generation {expected}, got {stop_generation}"),
        ));
    }
    if stop_generation > CYCLE5_ARM_STORE_GENERATION_TOTAL_V1 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_interval_stop",
            format!(
                "stop generation {stop_generation} is past the program end {CYCLE5_ARM_STORE_GENERATION_TOTAL_V1}"
            ),
        ));
    }
    if checkpoint_segment_updates == 0 || !updates.is_multiple_of(checkpoint_segment_updates) {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_interval_stop",
            "the training window must be a whole number of checkpoint segments",
        ));
    }
    // A preflight prefix runs one or more short windows inside the genesis
    // interval and never chains a manifest, so it is pinned to the genesis
    // manifest and bounded below the first refresh boundary rather than
    // matched to a manifest position it does not have.
    if contract.refresh_index != 0 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_preflight_manifest_advanced",
            format!(
                "a preflight runs only against the genesis manifest, got refresh index {}",
                contract.refresh_index
            ),
        ));
    }
    if stop_generation > CYCLE5_REFRESH_INTERVAL_V1 {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_preflight_manifest_advanced",
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

/// Runs exactly one cycle-5 refresh interval for one arm and returns.
///
/// # Errors
///
/// Returns a classified [`Cycle5ArmErrorV1`]: `Contract` for any contract,
/// manifest, locator, or chain rejection (bin exit code 3) and `Runtime` for
/// an I/O or training failure (bin exit code 1).
#[allow(clippy::too_many_lines)]
pub fn run_native_cycle5_arm_v1(request: &Cycle5ArmRequestV1) -> Result<Cycle5ArmOutcomeV1> {
    // 1. Run contract, arm-kind consistency, and the device contract.
    let run_bytes = std::fs::read(&request.run_record).map_err(|error| {
        Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_run_record_read",
            format!("{}: {error}", request.run_record.display()),
        )
    })?;
    let run = decode_train_run_v2(&run_bytes).map_err(|error| {
        Cycle5ArmErrorV1::contract("cycle5_arm_v1_run_record_rejected", error.to_string())
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
        Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_slot_locator_read",
            format!("{}: {error}", request.slot_locator.display()),
        )
    })?;
    let locator = decode_slot_locator_v1(&locator_bytes)?;
    let slot_store_roots = slot_store_roots_for_manifest_v1(&locator, &manifest)?;

    // 3. Open or bootstrap the Store, authoring genesis from the pinned
    //    parent checkpoint when the Store is new.
    let (parent_dir, root_basename) = store_root_parts_v1(&request.store_root)?;
    let mode = if request.preflight_updates.is_some() {
        Cycle5ArmStoreModeV1::Preflight
    } else {
        Cycle5ArmStoreModeV1::Formal
    };
    // Verify the prefix admits this mode before touching the Store, but do not
    // WRITE the marker yet. `formal` and `preflight` are terminal, so writing
    // one on a prefix that turns out to hold no genesis would strand it: the
    // operator's next `--bootstrap-genesis` would be refused by a marker this
    // run had no business leaving behind. An unseeded prefix has to come out
    // of a rejected interval exactly as it went in, still bootstrap-eligible.
    verify_store_mode_marker_v1(&parent_dir, request.arm, &run, mode)?;

    // Genesis is its own mode now. An interval invocation never authors one:
    // the genesis refresh manifest this invocation was handed can only have
    // been built AFTER the Store's genesis existed, so an unseeded Store here
    // means the two are out of order.
    //
    // Probed READ-ONLY, before anything is created.
    // `bootstrap_native_training_store_v2` is a mutating call -- it creates
    // the root, its lock and its directory skeleton, and clears a stale
    // run-record staging file -- so running it before the checks below would
    // let a rejected invocation leave recovery state behind. The bootstrap's
    // own inventory still has the final say further down; this is the
    // pre-filter that keeps the rejection side-effect-free.
    if !store_holds_published_genesis_v1(&request.store_root)? {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_genesis_not_bootstrapped",
            format!(
                "{} holds no genesis; run --bootstrap-genesis before the first interval",
                request.store_root.display()
            ),
        ));
    }
    // The last gate before ANY mutation: the run record must describe THIS
    // build. Everything above it is a pure read, so a provenance mismatch
    // leaves the Store root exactly as it found it.
    require_run_record_is_this_build_v1(&run)?;

    let bootstrapped =
        bootstrap_native_training_store_v2(&parent_dir, &root_basename).map_err(|error| {
            Cycle5ArmErrorV1::runtime("cycle5_arm_v1_bootstrap_failed", error.to_string())
        })?;
    // The authority, re-derived from the bootstrap's own inventory rather
    // than from the probe above.
    if !bootstrapped.latest_final_present() {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_genesis_not_bootstrapped",
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

    // 4. No baseline chain: the control arm runs the frozen v3 path. The
    //    chain directory still holds the origin record.

    // Verify-or-publish, on every open and for every arm kind (review
    // finding P2): the origin record must exist and must bind this run, arm,
    // parent checkpoint, and the Store's own genesis checkpoint, whether or
    // not this invocation is the one that authored genesis. Runs before the
    // chain is touched, so a chain directory paired with the wrong run or arm
    // fails closed before anything is published into it.
    let genesis_identity = genesis_identity_from_store_v1(&root, &run)?;
    ensure_origin_record_v1(&request.chain_dir, request.arm, &run, &genesis_identity)?;

    let engine = Arc::new(resolve_population_opponent_cycle5_v1(
        &manifest,
        &slot_store_roots,
        run.run_sha256(),
    )?);

    // 5. Train exactly one interval.
    let mut session: Option<NativeTrainingStoreContinuationSessionV2> = None;
    let mut resume_generation: Option<u64> = None;
    let latest_generation_index = loop {
        let resumed = resume_native_training_store_with_session_v2(
            &root,
            &run,
            execution_config.clone(),
            session.take(),
        )
        .map_err(|error| {
            Cycle5ArmErrorV1::runtime("cycle5_arm_v1_resume_failed", error.to_string())
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
                    return Err(Cycle5ArmErrorV1::contract(
                        "cycle5_arm_v1_interval_incomplete",
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
                continuation.executor.set_baseline_state_v4(None);
                let prepared = prepare_segment_v2(
                    &mut continuation.executor,
                    &run,
                    &continuation.parent_boundary,
                    &continuation.parent_checkpoint,
                )
                .map_err(|error| {
                    Cycle5ArmErrorV1::runtime("cycle5_arm_v1_prepare_failed", error.code().to_owned())
                })?;
                let (receipt, next_session) = publish_prepared_segment_with_session_v2(
                    &root,
                    &run,
                    &continuation.parent_boundary,
                    &continuation.parent_checkpoint,
                    &prepared,
                    &continuation.tip_proof,
                    continuation.windows_since_full_walk,
                )
                .map_err(|error| {
                    Cycle5ArmErrorV1::runtime(
                        "cycle5_arm_v1_publish_failed",
                        error.code().to_owned(),
                    )
                })?;
                prepared.commit_v2(receipt).map_err(|error| {
                    Cycle5ArmErrorV1::runtime(
                        "cycle5_arm_v1_commit_failed",
                        error.code().to_owned(),
                    )
                })?;
                session = Some(next_session);
            }
        }
    };

    // 6. Full-store validation on exit.
    let state = validate_native_training_store_v2(&root, &run).map_err(|error| {
        Cycle5ArmErrorV1::runtime("cycle5_arm_v1_validate_failed", error.to_string())
    })?;
    if state.latest_generation_index() != latest_generation_index {
        return Err(Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_validate_failed",
            "final validation disagrees with the trained tip",
        ));
    }

    Ok(Cycle5ArmOutcomeV1 {
        arm: request.arm,
        resume_generation_index: resume_generation.unwrap_or(latest_generation_index),
        latest_generation_index,
        trainee_local_generation: CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1
            .saturating_add(latest_generation_index),
        refresh_index: contract.refresh_index,
        refresh_manifest_sha256: lower_hex_raw32_v1(manifest.manifest_sha256_v1()),
        // No chain for the v3 control arm; kept so the outcome shape matches
        // the cycle-4 wrapper's expectations.
        baseline_chain_generation: None,
    })
}

/// The Store prefix's mode marker: which of the two mutually exclusive modes
/// (formal or preflight) first claimed this prefix, and for which arm and run.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cycle5ArmModeMarkerV1 {
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
enum Cycle5ArmStoreModeV1 {
    Bootstrap,
    Formal,
    Preflight,
}

impl Cycle5ArmStoreModeV1 {
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
enum Cycle5ArmStoreModeStateV1 {
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
    arm: Cycle5ArmKindV1,
    run: &ValidatedTrainRunV2,
    mode: Cycle5ArmStoreModeV1,
) -> Result<Cycle5ArmStoreModeStateV1> {
    let expected = Cycle5ArmModeMarkerV1 {
        schema: CYCLE5_ARM_MODE_MARKER_SCHEMA_V1.to_owned(),
        mode: mode.wire_v1().to_owned(),
        arm_kind: arm.wire_v1().to_owned(),
        run_sha256: run.run_sha256().to_owned(),
    };
    let bytes = to_canonical_json_bytes_v1(&expected, CanonicalJsonNullPolicyV1::Forbid).map_err(
        |error| Cycle5ArmErrorV1::runtime("cycle5_arm_v1_mode_marker", error.to_string()),
    )?;
    let path = parent_dir.join(CYCLE5_ARM_MODE_MARKER_FILENAME_V1);
    match std::fs::read(&path) {
        Ok(existing) => {
            if existing == bytes {
                return Ok(Cycle5ArmStoreModeStateV1::AlreadyClaimed);
            }
            let actual: Cycle5ArmModeMarkerV1 =
                serde_json::from_slice(&existing).map_err(|error| {
                    Cycle5ArmErrorV1::contract(
                        "cycle5_arm_v1_mode_marker_conflict",
                        format!("{} is unreadable: {error}", path.display()),
                    )
                })?;
            let conflict = || {
                Cycle5ArmErrorV1::contract(
                    "cycle5_arm_v1_mode_marker_conflict",
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
            if actual.mode == Cycle5ArmStoreModeV1::Bootstrap.wire_v1()
                && matches!(
                    mode,
                    Cycle5ArmStoreModeV1::Formal | Cycle5ArmStoreModeV1::Preflight
                )
            {
                return Ok(Cycle5ArmStoreModeStateV1::PromotableFromBootstrap);
            }
            Err(conflict())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(Cycle5ArmStoreModeStateV1::Absent)
        }
        Err(error) => Err(Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_mode_marker",
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
    arm: Cycle5ArmKindV1,
    run: &ValidatedTrainRunV2,
    mode: Cycle5ArmStoreModeV1,
) -> Result<()> {
    if verify_store_mode_marker_v1(parent_dir, arm, run, mode)?
        == Cycle5ArmStoreModeStateV1::AlreadyClaimed
    {
        return Ok(());
    }
    let expected = Cycle5ArmModeMarkerV1 {
        schema: CYCLE5_ARM_MODE_MARKER_SCHEMA_V1.to_owned(),
        mode: mode.wire_v1().to_owned(),
        arm_kind: arm.wire_v1().to_owned(),
        run_sha256: run.run_sha256().to_owned(),
    };
    let bytes = to_canonical_json_bytes_v1(&expected, CanonicalJsonNullPolicyV1::Forbid).map_err(
        |error| Cycle5ArmErrorV1::runtime("cycle5_arm_v1_mode_marker", error.to_string()),
    )?;
    std::fs::create_dir_all(parent_dir).map_err(|error| {
        Cycle5ArmErrorV1::runtime("cycle5_arm_v1_mode_marker", error.to_string())
    })?;
    write_file_atomically_v1(&parent_dir.join(CYCLE5_ARM_MODE_MARKER_FILENAME_V1), &bytes)
        .map_err(|error| Cycle5ArmErrorV1::runtime("cycle5_arm_v1_mode_marker", error.to_string()))
}

// ---------------------------------------------------------------------
// Genesis bootstrap
// ---------------------------------------------------------------------

/// One genesis bootstrap's complete, typed request. Deliberately NOT a
/// variant of [`Cycle5ArmRequestV1`]: a bootstrap takes no refresh manifest,
/// no payoff panel, no stop generation, and no preflight window, because
/// nothing about a training interval applies to it.
#[derive(Clone, Debug)]
pub struct Cycle5ArmBootstrapRequestV1 {
    pub arm: Cycle5ArmKindV1,
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
pub struct Cycle5ArmBootstrapOutcomeV1 {
    pub arm: Cycle5ArmKindV1,
    pub run_sha256: String,
    pub base_seed: u64,
    /// Always 0: the Store's own genesis generation.
    pub genesis_generation_index: u64,
    /// Always 2048: the same generation in the contract's trainee-local
    /// numbering, which is what the genesis manifest's own-run slot declares.
    pub trainee_local_generation: u64,
    pub genesis: Cycle5ArmGenesisIdentityV1,
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
/// Returns a classified [`Cycle5ArmErrorV1`]: `Contract` (bin exit code 3)
/// for any contract, locator, or already-trained-Store rejection, `Runtime`
/// (bin exit code 1) for an I/O or publication failure.
pub fn run_native_cycle5_arm_bootstrap_genesis_v1(
    request: &Cycle5ArmBootstrapRequestV1,
) -> Result<Cycle5ArmBootstrapOutcomeV1> {
    // 1. Exactly the run-contract, arm-kind, and device-contract validation a
    //    normal invocation performs, so a run record that could never train
    //    is rejected here rather than after a Store exists.
    let run_bytes = std::fs::read(&request.run_record).map_err(|error| {
        Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_run_record_read",
            format!("{}: {error}", request.run_record.display()),
        )
    })?;
    let run = decode_train_run_v2(&run_bytes).map_err(|error| {
        Cycle5ArmErrorV1::contract("cycle5_arm_v1_run_record_rejected", error.to_string())
    })?;
    validate_run_contract_v1(&run, request.arm)?;
    validate_device_contract_v1(&run, request.arm)?;

    // 2. The locator, for its genesis parent store root.
    let locator_bytes = std::fs::read(&request.slot_locator).map_err(|error| {
        Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_slot_locator_read",
            format!("{}: {error}", request.slot_locator.display()),
        )
    })?;
    let locator = decode_slot_locator_v1(&locator_bytes)?;

    // 3. Claim the prefix as bootstrapped, then open it. The build-provenance
    //    gate runs first: a Store must never be seeded by a build the run
    //    record does not describe.
    require_run_record_is_this_build_v1(&run)?;
    let (parent_dir, root_basename) = store_root_parts_v1(&request.store_root)?;
    claim_store_mode_marker_v1(
        &parent_dir,
        request.arm,
        &run,
        Cycle5ArmStoreModeV1::Bootstrap,
    )?;
    let bootstrapped =
        bootstrap_native_training_store_v2(&parent_dir, &root_basename).map_err(|error| {
            Cycle5ArmErrorV1::runtime("cycle5_arm_v1_bootstrap_failed", error.to_string())
        })?;
    let already_seeded = bootstrapped.latest_final_present();
    let root = bootstrapped.into_root();

    // 4. Publish genesis, unless a previous bootstrap already did. The Store
    //    commit and the origin-record write are two steps, so a crash between
    //    them leaves a generation-0 Store whose origin record is missing;
    //    this invocation finishes that publication instead of refusing it.
    let genesis = if already_seeded {
        let state = validate_native_training_store_v2(&root, &run).map_err(|error| {
            Cycle5ArmErrorV1::runtime("cycle5_arm_v1_validate_failed", error.to_string())
        })?;
        bootstrap_may_adopt_seeded_store_v1(state.latest_generation_index(), &request.store_root)?;
        let identity = genesis_identity_from_checkpoint_v1(state.latest_checkpoint());
        ensure_origin_record_v1(&request.chain_dir, request.arm, &run, &identity)?;
        identity
    } else {
        author_genesis_from_parent_v1(&root, &run, &locator, &request.chain_dir, request.arm)?
    };

    // 5. The same final-store validation an interval exit performs.
    let state = validate_native_training_store_v2(&root, &run).map_err(|error| {
        Cycle5ArmErrorV1::runtime("cycle5_arm_v1_validate_failed", error.to_string())
    })?;
    if state.latest_generation_index() != 0 {
        return Err(Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_validate_failed",
            "a bootstrap must leave the Store at generation 0",
        ));
    }

    Ok(Cycle5ArmBootstrapOutcomeV1 {
        arm: request.arm,
        run_sha256: run.run_sha256().to_owned(),
        base_seed: run.record().schedule().base_seed,
        genesis_generation_index: 0,
        trainee_local_generation: CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1,
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
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_genesis_already_present",
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
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_store_root",
                "--store-root must name a directory inside a parent directory",
            )
        })?;
    let basename = store_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_store_root",
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
    locator: &Cycle5SlotLocatorV1,
    chain_dir: &Path,
    arm: Cycle5ArmKindV1,
) -> Result<Cycle5ArmGenesisIdentityV1> {
    let declared = declared_origin_v1(run)?;
    let parent_root = locator.genesis_parent_store_root.as_ref().ok_or_else(|| {
        Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_genesis_parent_missing",
            "the slot locator carries no genesis_parent_store_root",
        )
    })?;
    let parent_root = PathBuf::from(parent_root);
    let staged = stage_ladder_checkpoint_initialization_v1(&parent_root, declared.generation)
        .map_err(|error| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_genesis_parent_rejected",
                format!("{}: {error}", parent_root.display()),
            )
        })?;
    if &staged != declared {
        return Err(Cycle5ArmErrorV1::contract(
            "cycle5_arm_v1_genesis_origin_mismatch",
            "the parent store does not reproduce the run record's pinned origin",
        ));
    }
    let checkpoint_ref = ladder_init_as_checkpoint_ref_v1(declared);
    let authority =
        resolve_ladder_checkpoint_authority_v1(&parent_root, &checkpoint_ref).map_err(|error| {
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_genesis_parent_rejected",
                format!("{}: {error}", parent_root.display()),
            )
        })?;
    let (parent_checkpoint, parent_payload) = authority.into_checkpoint_and_payload();
    let genesis_error =
        |detail: String| Cycle5ArmErrorV1::runtime("cycle5_arm_v1_genesis_failed", detail);
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

/// The run record's pinned genesis origin, which every cycle-5 arm must
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
            Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_genesis_requires_origin",
                "a cycle-5 arm's genesis must be seeded from a pinned parent checkpoint",
            )
        })
}

/// The four genesis-checkpoint hashes the origin record carries, read off one
/// already-validated checkpoint manifest.
fn genesis_identity_from_checkpoint_v1(
    checkpoint: &CheckpointManifestV3,
) -> Cycle5ArmGenesisIdentityV1 {
    Cycle5ArmGenesisIdentityV1 {
        checkpoint_manifest_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_manifest_sha256()),
        checkpoint_payload_sha256: lower_hex_raw32_v1(checkpoint.checkpoint_payload_sha256()),
        model_parameter_sha256: lower_hex_raw32_v1(checkpoint.model_parameter_sha256()),
        train_state_sha256: lower_hex_raw32_v1(checkpoint.train_state_sha256()),
    }
}

/// The same four hashes, taken from an already-seeded Store's own generation
/// 0 through the ordinary validated boundary walk. This is how an invocation
/// that did NOT author genesis reconstructs what the origin record must say.
/// Generation 0 carries no update evidence, so the plain walk resolves it.
fn genesis_identity_from_store_v1(
    root: &ValidatedNativeTrainingStoreRootV2,
    run: &ValidatedTrainRunV2,
) -> Result<Cycle5ArmGenesisIdentityV1> {
    let boundary = load_native_training_boundary_v2(root, run, 0).map_err(|error| {
        Cycle5ArmErrorV1::runtime(
            "cycle5_arm_v1_genesis_identity",
            format!("store generation 0: {error}"),
        )
    })?;
    Ok(genesis_identity_from_checkpoint_v1(boundary.checkpoint()))
}

/// Read-only binding check for a consumer that resolves a cycle-5 Store
/// through its origin directory without owning it:
/// the chain directory's origin record must exist, decode, and name this
/// run and this Store's genesis checkpoint. Every field is compared: schema,
/// arm kind when the run declares one, run hash, base seed, the declared
/// parent checkpoint pin, and the four genesis hashes against `genesis`, the
/// Store's generation-0 manifest, which the caller resolves through the
/// genesis decode path (no Store walk). Publishes nothing.
pub(crate) fn verify_origin_record_binds_run_v1(
    chain_dir: &Path,
    run: &ValidatedTrainRunV2,
    genesis: &CheckpointManifestV3,
) -> std::result::Result<(), &'static str> {
    verify_origin_record_binds_v1(
        chain_dir,
        run,
        &genesis_identity_from_checkpoint_v1(genesis),
    )
}

fn verify_origin_record_binds_v1(
    chain_dir: &Path,
    run: &ValidatedTrainRunV2,
    genesis: &Cycle5ArmGenesisIdentityV1,
) -> std::result::Result<(), &'static str> {
    let path = chain_dir.join(CYCLE5_ARM_ORIGIN_RECORD_FILENAME_V1);
    let bytes = std::fs::read(&path).map_err(|_| "cycle5_arm_v1_origin_record_missing")?;
    let decoded: Cycle5ArmOriginRecordV1 =
        from_canonical_json_bytes_v1(&bytes, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| "cycle5_arm_v1_origin_record_undecodable")?;
    let contracts = run.record().contracts();
    let declared_arm = contracts
        .population_program_v2_cycle5
        .as_ref()
        .map(|program| program.arm_kind.as_str());
    let Some(declared) = contracts.opponent_ladder_initialization.as_ref() else {
        return Err("cycle5_arm_v1_origin_record_run_declares_no_parent");
    };
    let binds = decoded.schema == CYCLE5_ARM_ORIGIN_RECORD_SCHEMA_V1
        && declared_arm.is_none_or(|arm| decoded.arm_kind == arm)
        && decoded.run_sha256 == run.run_sha256()
        && decoded.base_seed == run.record().schedule().base_seed
        && decoded.init_generation == declared.generation
        && decoded.parent_source_run_sha256 == declared.source_run_sha256
        && decoded.parent_checkpoint_sha256 == declared.checkpoint_sha256
        && decoded.parent_sidecar_sha256 == declared.sidecar_sha256
        && decoded.parent_state_sha256 == declared.state_sha256
        && decoded.derived_model_parameter_sha256 == declared.derived_model_parameter_sha256
        && decoded.genesis_checkpoint_manifest_sha256 == genesis.checkpoint_manifest_sha256
        && decoded.genesis_checkpoint_payload_sha256 == genesis.checkpoint_payload_sha256
        && decoded.genesis_model_parameter_sha256 == genesis.model_parameter_sha256
        && decoded.genesis_train_state_sha256 == genesis.train_state_sha256;
    if binds {
        Ok(())
    } else {
        Err("cycle5_arm_v1_origin_record_binds_another_run")
    }
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
    arm: Cycle5ArmKindV1,
    run: &ValidatedTrainRunV2,
    identity: &Cycle5ArmGenesisIdentityV1,
) -> Result<()> {
    let declared = declared_origin_v1(run)?;
    let expected = Cycle5ArmOriginRecordV1 {
        schema: CYCLE5_ARM_ORIGIN_RECORD_SCHEMA_V1.to_owned(),
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
        |error| Cycle5ArmErrorV1::runtime("cycle5_arm_v1_origin_record", error.to_string()),
    )?;
    let path = chain_dir.join(CYCLE5_ARM_ORIGIN_RECORD_FILENAME_V1);
    if let Ok(existing) = std::fs::read(&path) {
        let decoded: Cycle5ArmOriginRecordV1 =
            from_canonical_json_bytes_v1(&existing, CanonicalJsonNullPolicyV1::Forbid).map_err(
                |error| {
                    Cycle5ArmErrorV1::contract(
                        "cycle5_arm_v1_origin_record_conflict",
                        format!("the existing origin record does not decode: {error}"),
                    )
                },
            )?;
        if decoded != expected {
            return Err(Cycle5ArmErrorV1::contract(
                "cycle5_arm_v1_origin_record_conflict",
                "the existing origin record does not bind this run, arm, parent checkpoint, and genesis checkpoint",
            ));
        }
        return Ok(());
    }
    std::fs::create_dir_all(chain_dir).map_err(|error| {
        Cycle5ArmErrorV1::runtime("cycle5_arm_v1_origin_record", error.to_string())
    })?;
    write_file_atomically_v1(&path, &bytes).map_err(|error| {
        Cycle5ArmErrorV1::runtime("cycle5_arm_v1_origin_record", error.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_population_refresh_manifest_cycle5_v1::{
        build_cycle5_refresh_manifest_v1, FrozenOccupantIdentityCycle5V1, CYCLE5_ANCHOR_0_V1,
        CYCLE5_ANCHOR_1_V1, CYCLE5_CURRENT_0_V1, CYCLE5_CYCLE3_LINEAGE_BASE_SEED_V1,
        CYCLE5_CYCLE3_LINEAGE_RUN_SHA256_V1, CYCLE5_EXPLOITER_0_V1, CYCLE5_EXPLOITER_1_V1,
        CYCLE5_GENESIS_SLOT_WEIGHT_UNITS_V1, CYCLE5_HISTORICAL_1_ROTATION_V1,
        CYCLE5_HISTORICAL_LAG_V1,
    };
    use crate::native_training_store_run_v2::{
        test_fixture_bytes_population_program_v2_cycle5_seeded_v1,
        test_fixture_bytes_population_program_v2_cycle5_v1, test_fixture_bytes_v2,
    };

    const ROLES_V1: [&str; CYCLE5_SLOT_COUNT_V1] = [
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
    /// test's exclusive use (the convention `cycle5_refresh_build_v1.rs`'s
    /// own tests follow). The caller removes it when done.
    fn fresh_temp_dir_v1(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mtg-kernel-cycle5-arm-{label}-{}-{nonce}",
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
        frozen: &FrozenOccupantIdentityCycle5V1,
        weight_units: u64,
    ) -> Cycle5RefreshSlotV1 {
        Cycle5RefreshSlotV1 {
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
    ) -> Cycle5RefreshSlotV1 {
        Cycle5RefreshSlotV1 {
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
    ) -> Vec<Cycle5RefreshSlotV1> {
        let weight = CYCLE5_GENESIS_SLOT_WEIGHT_UNITS_V1;
        let trainee_local_generation =
            CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1 + refresh_index * CYCLE5_REFRESH_INTERVAL_V1;
        let historical_generation = trainee_local_generation - CYCLE5_HISTORICAL_LAG_V1;
        let (historical_seed, historical_run) = if refresh_index <= 3 {
            (
                CYCLE5_CYCLE3_LINEAGE_BASE_SEED_V1,
                CYCLE5_CYCLE3_LINEAGE_RUN_SHA256_V1.to_owned(),
            )
        } else {
            (trainee_base_seed, trainee_run_sha256.to_owned())
        };
        let rotation = &CYCLE5_HISTORICAL_1_ROTATION_V1[(refresh_index % 3) as usize];
        vec![
            frozen_slot_v1(0, &CYCLE5_ANCHOR_0_V1, weight),
            frozen_slot_v1(1, &CYCLE5_ANCHOR_1_V1, weight),
            derived_slot_v1(
                2,
                historical_seed,
                &historical_run,
                historical_generation,
                21,
                weight,
            ),
            frozen_slot_v1(3, rotation, weight),
            frozen_slot_v1(4, &CYCLE5_CURRENT_0_V1, weight),
            derived_slot_v1(
                5,
                trainee_base_seed,
                trainee_run_sha256,
                trainee_local_generation,
                55,
                weight,
            ),
            frozen_slot_v1(6, &CYCLE5_EXPLOITER_0_V1, weight),
            frozen_slot_v1(7, &CYCLE5_EXPLOITER_1_V1, weight),
        ]
    }

    fn genesis_manifest_for_v1(
        trainee_run_sha256: &str,
        trainee_base_seed: u64,
    ) -> Cycle5RefreshManifestV1 {
        build_cycle5_refresh_manifest_v1(
            0,
            None,
            None,
            trainee_run_sha256,
            trainee_base_seed,
            manifest_slots_v1(0, trainee_run_sha256, trainee_base_seed),
        )
        .expect("genesis manifest must build")
    }

    fn genesis_manifest_v1() -> Cycle5RefreshManifestV1 {
        genesis_manifest_for_v1(&foreign_run_sha256_v1(), FOREIGN_BASE_SEED_V1)
    }

    fn refresh_one_manifest_for_v1(
        genesis: &Cycle5RefreshManifestV1,
        panel_bytes: &[u8],
        trainee_run_sha256: &str,
        trainee_base_seed: u64,
    ) -> Cycle5RefreshManifestV1 {
        build_cycle5_refresh_manifest_v1(
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
        genesis: &Cycle5RefreshManifestV1,
        panel_bytes: &[u8],
    ) -> Cycle5RefreshManifestV1 {
        refresh_one_manifest_for_v1(
            genesis,
            panel_bytes,
            &foreign_run_sha256_v1(),
            FOREIGN_BASE_SEED_V1,
        )
    }

    fn locator_for_v1(manifest: &Cycle5RefreshManifestV1) -> Cycle5SlotLocatorV1 {
        Cycle5SlotLocatorV1 {
            schema: CYCLE5_ARM_SLOT_LOCATOR_SCHEMA_V1.to_owned(),
            stores: manifest
                .slots_v1()
                .iter()
                .map(|slot| Cycle5SlotLocatorEntryV1 {
                    checkpoint_manifest_sha256: slot.checkpoint_manifest_sha256.clone(),
                    store_root: format!("D:/cycle5/slot-{}", slot.slot_index),
                })
                .collect(),
            genesis_parent_store_root: None,
        }
    }

    fn run_for_arm_v1(arm: Cycle5ArmKindV1) -> ValidatedTrainRunV2 {
        let bytes = test_fixture_bytes_population_program_v2_cycle5_v1(arm.wire_v1());
        decode_train_run_v2(&bytes).expect("cycle-5 fixture must validate")
    }

    /// The same arm record, carrying the `opponent_ladder_initialization`
    /// section a real arm declares. Every Store that reached an interval was
    /// bootstrapped from that section, so the origin record can always be
    /// rebuilt from it.
    fn seeded_run_for_arm_v1(arm: Cycle5ArmKindV1) -> ValidatedTrainRunV2 {
        let bytes = test_fixture_bytes_population_program_v2_cycle5_seeded_v1(arm.wire_v1());
        decode_train_run_v2(&bytes).expect("seeded cycle-5 fixture must validate")
    }

    fn genesis_identity_fixture_v1(nonce: u8) -> Cycle5ArmGenesisIdentityV1 {
        Cycle5ArmGenesisIdentityV1 {
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
    fn the_control_arm_accepts_its_own_run_record_v1() {
        let run = run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        validate_run_contract_v1(&run, Cycle5ArmKindV1::ControlV3)
            .expect("matching arm must validate");
    }

    #[test]
    fn arm_flag_must_match_the_run_record_v1() {
        let run = run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let error = validate_run_contract_v1(&run, Cycle5ArmKindV1::CenteredV5)
            .expect_err("mismatch must fail closed");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(error.code_v1(), "cycle5_arm_v1_arm_kind_mismatch");
    }

    #[test]
    fn the_centered_arm_is_refused_as_unratified_v1() {
        // A record that names centered-v5 cannot even decode while the v5
        // contract is unratified (the V2 validator refuses the section), so
        // the launcher-level refusal is proven on the arm kind's own gates.
        assert!(!Cycle5ArmKindV1::ControlV3.uses_centered_baseline_v1());
        assert!(Cycle5ArmKindV1::CenteredV5.uses_centered_baseline_v1());
        assert!(!Cycle5ArmKindV1::CenteredV5.static_pool_v1());
        assert_eq!(Cycle5ArmKindV1::from_wire_v1("centered-v5"), Some(Cycle5ArmKindV1::CenteredV5));
        assert_eq!(Cycle5ArmKindV1::from_wire_v1("control-r"), None);
        assert_eq!(Cycle5ArmKindV1::from_wire_v1("treatment-rb"), None);
    }

    #[test]
    fn a_v4_trainer_section_is_refused_on_any_cycle5_arm_v1() {
        let run = run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        assert!(run.record().contracts().trainer_v4_candidate.is_none());
        assert!(run.record().contracts().trainer_v5_candidate.is_none());
    }

    #[test]
    fn a_run_without_the_cycle5_section_is_rejected_v1() {
        let bytes = crate::native_training_store_run_v2::test_fixture_bytes_v2();
        let run = decode_train_run_v2(&bytes).expect("v3 fixture validates");
        let error = validate_run_contract_v1(&run, Cycle5ArmKindV1::ControlV3)
            .expect_err("a v3 run is not a cycle-5 arm");
        assert_eq!(
            error.code_v1(),
            "cycle5_arm_v1_missing_population_program_v2_cycle5"
        );
    }

    #[test]
    fn the_control_arm_keeps_its_own_frozen_backend_v1() {
        let control = run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let backend = validate_device_contract_v1(&control, Cycle5ArmKindV1::ControlV3)
            .expect("control-v3 keeps its own frozen backend");
        assert_ne!(backend, NativeTrainingNumericalBackendV1::CudaBurnDense);
    }

    #[test]
    fn execution_config_is_derived_from_the_run_record_v1() {
        let run = run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let backend =
            validate_device_contract_v1(&run, Cycle5ArmKindV1::ControlV3).expect("backend");
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
    fn a_read_only_consumer_requires_the_origin_record_to_bind_run_and_genesis_v1() {
        let dir = fresh_temp_dir_v1("origin-verify-readonly");
        let run = seeded_run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let identity = genesis_identity_fixture_v1(1);
        assert_eq!(
            verify_origin_record_binds_v1(&dir, &run, &identity),
            Err("cycle5_arm_v1_origin_record_missing")
        );
        ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &run, &identity)
            .expect("publish");
        verify_origin_record_binds_v1(&dir, &run, &identity)
            .expect("the published record binds its own run and genesis");
        // The same record against a genesis whose payload hash differs is refused.
        let mut other_genesis = identity.clone();
        other_genesis.checkpoint_payload_sha256 =
            genesis_identity_fixture_v1(2).checkpoint_payload_sha256;
        assert_eq!(
            verify_origin_record_binds_v1(&dir, &run, &other_genesis),
            Err("cycle5_arm_v1_origin_record_binds_another_run")
        );
        // Another run against the same record is refused.
        let other_run = seeded_run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        assert_eq!(
            verify_origin_record_binds_v1(&dir, &other_run, &identity),
            Err("cycle5_arm_v1_origin_record_binds_another_run")
        );
        // A record whose stored genesis train-state hash was tampered is refused.
        let path = dir.join(CYCLE5_ARM_ORIGIN_RECORD_FILENAME_V1);
        let mut decoded: Cycle5ArmOriginRecordV1 = from_canonical_json_bytes_v1(
            &std::fs::read(&path).expect("read"),
            CanonicalJsonNullPolicyV1::Forbid,
        )
        .expect("decode");
        decoded.genesis_train_state_sha256 = genesis_identity_fixture_v1(2).train_state_sha256;
        std::fs::write(
            &path,
            to_canonical_json_bytes_v1(&decoded, CanonicalJsonNullPolicyV1::Forbid)
                .expect("encode"),
        )
        .expect("write");
        assert_eq!(
            verify_origin_record_binds_v1(&dir, &run, &identity),
            Err("cycle5_arm_v1_origin_record_binds_another_run")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_origin_record_is_published_when_the_chain_has_none_v1() {
        let dir = fresh_temp_dir_v1("origin-publish");
        let run = seeded_run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let identity = genesis_identity_fixture_v1(1);
        let path = dir.join(CYCLE5_ARM_ORIGIN_RECORD_FILENAME_V1);
        assert!(!path.exists());
        ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &run, &identity)
            .expect("a missing origin record is published");
        assert!(path.is_file(), "the record must use the contract's name");
        let decoded: Cycle5ArmOriginRecordV1 = from_canonical_json_bytes_v1(
            &std::fs::read(&path).expect("read"),
            CanonicalJsonNullPolicyV1::Forbid,
        )
        .expect("the published record decodes");
        assert_eq!(decoded.schema, CYCLE5_ARM_ORIGIN_RECORD_SCHEMA_V1);
        assert_eq!(decoded.run_sha256, run.run_sha256());
        assert_eq!(decoded.arm_kind, Cycle5ArmKindV1::ControlV3.wire_v1());
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
        ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &run, &identity)
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
        let run = seeded_run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let identity = genesis_identity_fixture_v1(1);
        ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &run, &identity)
            .expect("publish");
        // A different genesis checkpoint under the same run and arm.
        let error = ensure_origin_record_v1(
            &dir,
            Cycle5ArmKindV1::ControlV3,
            &run,
            &genesis_identity_fixture_v1(2),
        )
        .expect_err("a different genesis checkpoint is a conflict");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(error.code_v1(), "cycle5_arm_v1_origin_record_conflict");
        // A different arm kind on the same chain directory.
        let error = ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &run, &identity)
            .expect_err("a different arm is a conflict");
        assert_eq!(error.code_v1(), "cycle5_arm_v1_origin_record_conflict");
        // A different run identity.
        let other_run = seeded_run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        assert_ne!(other_run.run_sha256(), run.run_sha256());
        let error = ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &other_run, &identity)
            .expect_err("a different run is a conflict");
        assert_eq!(error.code_v1(), "cycle5_arm_v1_origin_record_conflict");
        // Bytes that are not an origin record at all.
        let path = dir.join(CYCLE5_ARM_ORIGIN_RECORD_FILENAME_V1);
        std::fs::write(&path, b"{}").expect("write");
        let error = ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &run, &identity)
            .expect_err("an undecodable record is a conflict");
        assert_eq!(error.code_v1(), "cycle5_arm_v1_origin_record_conflict");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_run_without_a_pinned_origin_cannot_have_an_origin_record_v1() {
        // The unseeded fixture declares no `opponent_ladder_initialization`,
        // so there is nothing to restate; every arm kind fails closed.
        let dir = fresh_temp_dir_v1("origin-unpinned");
        for arm in [
            Cycle5ArmKindV1::ControlV3,
            Cycle5ArmKindV1::ControlV3,
            Cycle5ArmKindV1::ControlV3,
        ] {
            let run = run_for_arm_v1(arm);
            let error = ensure_origin_record_v1(&dir, arm, &run, &genesis_identity_fixture_v1(3))
                .expect_err("no pinned parent, no origin record");
            assert_eq!(error.code_v1(), "cycle5_arm_v1_genesis_requires_origin");
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
        let store_root = PathBuf::from("D:/cycle5/arm-a/store");
        bootstrap_may_adopt_seeded_store_v1(0, &store_root)
            .expect("a genesis-only Store is exactly what an interrupted bootstrap leaves");
        for generation in [1_u64, 4, 128, 2048] {
            let error = bootstrap_may_adopt_seeded_store_v1(generation, &store_root)
                .expect_err("a trained Store is never adopted by a bootstrap");
            assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
            assert_eq!(error.exit_code_v1(), 3);
            assert_eq!(error.code_v1(), "cycle5_arm_v1_genesis_already_present");
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
        let run = seeded_run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let identity = genesis_identity_fixture_v1(7);
        let path = dir.join(CYCLE5_ARM_ORIGIN_RECORD_FILENAME_V1);
        assert!(!path.exists(), "the lost process never wrote it");
        bootstrap_may_adopt_seeded_store_v1(0, Path::new("D:/cycle5/arm-a/store"))
            .expect("adoptable");
        ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &run, &identity)
            .expect("the retry finishes the publication");
        assert!(path.is_file());
        // And the interval path that follows agrees with what it published.
        ensure_origin_record_v1(&dir, Cycle5ArmKindV1::ControlV3, &run, &identity)
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
        assert_eq!(roots.len(), CYCLE5_SLOT_COUNT_V1);
        for (index, root) in roots.iter().enumerate() {
            assert_eq!(root, &PathBuf::from(format!("D:/cycle5/slot-{index}")));
        }
    }

    #[test]
    fn locator_roster_mismatch_fails_closed_v1() {
        let manifest = genesis_manifest_v1();
        let mut locator = locator_for_v1(&manifest);
        locator.stores[3].checkpoint_manifest_sha256 = digest_v1(7, 7);
        let error = slot_store_roots_for_manifest_v1(&locator, &manifest)
            .expect_err("a substituted identity must fail closed");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(
            error.code_v1(),
            "cycle5_arm_v1_slot_locator_roster_mismatch"
        );
    }

    #[test]
    fn locator_rejects_wrong_schema_count_duplicates_and_relative_paths_v1() {
        let manifest = genesis_manifest_v1();
        let encode = |locator: &Cycle5SlotLocatorV1| serde_json::to_vec(locator).expect("encode");

        let mut wrong_schema = locator_for_v1(&manifest);
        wrong_schema.schema = "mtg-kernel-cycle5-slot-locator/v1".to_owned();
        assert_eq!(
            decode_slot_locator_v1(&encode(&wrong_schema))
                .expect_err("schema")
                .code_v1(),
            "cycle5_arm_v1_slot_locator_schema"
        );

        let mut short = locator_for_v1(&manifest);
        short.stores.pop();
        assert_eq!(
            decode_slot_locator_v1(&encode(&short))
                .expect_err("count")
                .code_v1(),
            "cycle5_arm_v1_slot_locator_slot_count"
        );

        let mut duplicate = locator_for_v1(&manifest);
        duplicate.stores[1].checkpoint_manifest_sha256 =
            duplicate.stores[0].checkpoint_manifest_sha256.clone();
        assert_eq!(
            decode_slot_locator_v1(&encode(&duplicate))
                .expect_err("duplicate")
                .code_v1(),
            "cycle5_arm_v1_slot_locator_duplicate_identity"
        );

        let mut relative = locator_for_v1(&manifest);
        relative.stores[2].store_root = "slots/two".to_owned();
        assert_eq!(
            decode_slot_locator_v1(&encode(&relative))
                .expect_err("relative")
                .code_v1(),
            "cycle5_arm_v1_slot_locator_relative_path"
        );

        let mut malformed_identity = locator_for_v1(&manifest);
        malformed_identity.stores[4].checkpoint_manifest_sha256 = "not-a-digest".to_owned();
        assert_eq!(
            decode_slot_locator_v1(&encode(&malformed_identity))
                .expect_err("identity")
                .code_v1(),
            "cycle5_arm_v1_slot_locator_identity"
        );

        let good = locator_for_v1(&manifest);
        decode_slot_locator_v1(&encode(&good)).expect("a well-formed locator decodes");
    }

    // ------------------------------------------------------------------
    // Read-only slot-locator decode check (round F item 3)
    // ------------------------------------------------------------------

    /// A locator naming eight synthetic Store roots, each holding a real,
    /// decodable `run.json`, plus a ninth for the genesis parent. Nothing
    /// else of a Store is created: `--check-slot-locator` reads `run.json`
    /// and nothing else, and this fixture proves that by supplying nothing
    /// else.
    fn check_locator_fixture_v1(dir: &Path, parent: bool) -> (PathBuf, Cycle5SlotLocatorV1) {
        let manifest = genesis_manifest_v1();
        let mut locator = locator_for_v1(&manifest);
        let run_bytes = test_fixture_bytes_v2();
        for (index, entry) in locator.stores.iter_mut().enumerate() {
            let store_root = dir.join(format!("slot-{index}"));
            std::fs::create_dir_all(&store_root).expect("slot store root");
            std::fs::write(store_root.join("run.json"), &run_bytes).expect("slot run.json");
            entry.store_root = store_root.to_string_lossy().into_owned();
        }
        if parent {
            let parent_root = dir.join("genesis-parent");
            std::fs::create_dir_all(&parent_root).expect("parent store root");
            std::fs::write(parent_root.join("run.json"), &run_bytes).expect("parent run.json");
            locator.genesis_parent_store_root = Some(parent_root.to_string_lossy().into_owned());
        }
        let path = dir.join("slot-locator.json");
        std::fs::write(&path, serde_json::to_vec(&locator).expect("encode")).expect("locator");
        (path, locator)
    }

    #[test]
    fn check_slot_locator_decodes_every_slot_and_the_parent_v1() {
        let dir = fresh_temp_dir_v1("check-slot-locator-ok");
        let (path, _) = check_locator_fixture_v1(&dir, true);
        let outcome = run_native_cycle5_arm_check_slot_locator_v1(&path)
            .expect("a locator of decodable records must pass");
        assert_eq!(
            outcome,
            Cycle5SlotLocatorCheckOutcomeV1 {
                decoded_run_record_count: CYCLE5_SLOT_COUNT_V1 + 1,
                genesis_parent_checked: true,
            }
        );
        // Read-only: the check created no Store artifact of its own beside
        // the one run.json each fixture root holds.
        for index in 0..CYCLE5_SLOT_COUNT_V1 {
            let store_root = dir.join(format!("slot-{index}"));
            let entries = std::fs::read_dir(&store_root)
                .expect("slot root readable")
                .count();
            assert_eq!(entries, 1, "the check must not write into a Store root");
        }
    }

    #[test]
    fn check_slot_locator_without_a_parent_checks_only_the_slots_v1() {
        let dir = fresh_temp_dir_v1("check-slot-locator-no-parent");
        let (path, _) = check_locator_fixture_v1(&dir, false);
        let outcome =
            run_native_cycle5_arm_check_slot_locator_v1(&path).expect("no parent is not an error");
        assert_eq!(
            outcome,
            Cycle5SlotLocatorCheckOutcomeV1 {
                decoded_run_record_count: CYCLE5_SLOT_COUNT_V1,
                genesis_parent_checked: false,
            }
        );
    }

    #[test]
    fn check_slot_locator_fails_closed_on_an_undecodable_slot_record_v1() {
        let dir = fresh_temp_dir_v1("check-slot-locator-bad-slot");
        let (path, locator) = check_locator_fixture_v1(&dir, true);
        // The shape of the defect round F item 1 fixed: a key this build's
        // `deny_unknown_fields` schema does not know, which is exactly what
        // an unreadable roster Store looks like from here. Injected in
        // canonical key order ("a_" sorts before "artifact_schemas"), so the
        // rejection is the schema one and not an incidental complaint about
        // key ordering.
        let victim = Path::new(&locator.stores[5].store_root).join("run.json");
        let text = String::from_utf8(std::fs::read(&victim).expect("read")).expect("utf8");
        let injected = text.replacen(
            r#"{"artifact_schemas":"#,
            r#"{"a_round_f_unknown_key":1,"artifact_schemas":"#,
            1,
        );
        assert_ne!(injected, text, "the injection must actually apply");
        std::fs::write(&victim, injected).expect("write");
        let error = run_native_cycle5_arm_check_slot_locator_v1(&path)
            .expect_err("an undecodable slot record must fail closed");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(error.exit_code_v1(), 3);
        assert_eq!(
            error.code_v1(),
            "cycle5_arm_v1_slot_locator_store_run_rejected"
        );
    }

    #[test]
    fn check_slot_locator_fails_closed_on_a_missing_store_v1() {
        let dir = fresh_temp_dir_v1("check-slot-locator-missing");
        let (path, locator) = check_locator_fixture_v1(&dir, true);
        std::fs::remove_file(Path::new(&locator.stores[0].store_root).join("run.json"))
            .expect("remove");
        let error = run_native_cycle5_arm_check_slot_locator_v1(&path)
            .expect_err("a missing slot record must fail closed");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(error.exit_code_v1(), 3);
        assert_eq!(error.code_v1(), "cycle5_arm_v1_slot_locator_store_run_read");
    }

    #[test]
    fn check_slot_locator_fails_closed_on_an_undecodable_parent_record_v1() {
        let dir = fresh_temp_dir_v1("check-slot-locator-bad-parent");
        let (path, locator) = check_locator_fixture_v1(&dir, true);
        let parent = Path::new(
            locator
                .genesis_parent_store_root
                .as_ref()
                .expect("parent present"),
        )
        .join("run.json");
        std::fs::write(&parent, b"{}\n").expect("write");
        let error = run_native_cycle5_arm_check_slot_locator_v1(&path)
            .expect_err("an undecodable parent record must fail closed");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(
            error.code_v1(),
            "cycle5_arm_v1_slot_locator_store_run_rejected"
        );
    }

    #[test]
    fn check_slot_locator_fails_closed_on_a_missing_locator_v1() {
        let dir = fresh_temp_dir_v1("check-slot-locator-absent");
        let error = run_native_cycle5_arm_check_slot_locator_v1(&dir.join("nothing-here.json"))
            .expect_err("an absent locator must fail closed");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(error.exit_code_v1(), 3);
        assert_eq!(error.code_v1(), "cycle5_arm_v1_slot_locator_read");
    }

    // ------------------------------------------------------------------
    // Own-run generation translation (trainee-local label -> Store)
    // ------------------------------------------------------------------

    #[test]
    fn own_run_slot_labels_translate_by_the_2048_offset_v1() {
        // The manifest labels every slot trainee-locally; the arm's own
        // Store counts 0..=2048 for 896..=2944, so an own-run label of
        // `896 + n` is read at Store generation `n`.
        let arm_run = foreign_run_sha256_v1();
        for refresh_index in [0_u64, 1, 4, 16] {
            let slots = manifest_slots_v1(refresh_index, &arm_run, FOREIGN_BASE_SEED_V1);
            let program_update = refresh_index * CYCLE5_REFRESH_INTERVAL_V1;
            // current-1 is own-run at every index.
            let current_1 = &slots[5];
            assert_eq!(
                current_1.source_generation,
                CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1 + program_update,
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
                CYCLE5_CYCLE3_LINEAGE_RUN_SHA256_V1
            );
            let label = CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1
                + refresh_index * CYCLE5_REFRESH_INTERVAL_V1
                - CYCLE5_HISTORICAL_LAG_V1;
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
            CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1 + 4 * CYCLE5_REFRESH_INTERVAL_V1
                - CYCLE5_HISTORICAL_LAG_V1
        );
        assert_eq!(
            store_generation_for_slot_v1(historical_0, &arm_run).expect("own run"),
            4 * CYCLE5_REFRESH_INTERVAL_V1 - CYCLE5_HISTORICAL_LAG_V1,
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
            CYCLE5_ANCHOR_0_V1.source_generation
        );
        assert_eq!(
            slots[1].source_generation,
            CYCLE5_ANCHOR_1_V1.source_generation
        );
        assert_eq!(
            slots[4].source_generation,
            CYCLE5_CURRENT_0_V1.source_generation
        );
    }

    #[test]
    fn an_own_run_label_below_the_program_start_fails_closed_v1() {
        let arm_run = foreign_run_sha256_v1();
        let mut slots = manifest_slots_v1(1, &arm_run, FOREIGN_BASE_SEED_V1);
        for label in [0_u64, 1, CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1 - 1] {
            slots[5].source_generation = label;
            let error = store_generation_for_slot_v1(&slots[5], &arm_run)
                .expect_err("no store generation can carry this label");
            assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
            assert_eq!(error.code_v1(), "cycle5_arm_v1_own_run_slot_generation");
        }
        // The boundary case is admissible: 896 is the arm's own genesis.
        slots[5].source_generation = CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1;
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
        let roots: Vec<PathBuf> = (0..CYCLE5_SLOT_COUNT_V1)
            .map(|index| PathBuf::from(format!("D:/cycle5/absent-slot-{index}")))
            .collect();
        let error = resolve_population_opponent_cycle5_v1(&genesis, &roots, &arm_run, None)
            .expect_err("a store that carries nothing cannot occupy a slot");
        assert_eq!(error.code_v1(), "cycle5_arm_v1_population_run_read");
    }

    // ------------------------------------------------------------------
    // Refresh manifest decode and the static-pool rule
    // ------------------------------------------------------------------

    fn write_chain_v1(dir: &Path, genesis: &Cycle5RefreshManifestV1) -> PathBuf {
        let path = dir.join(cycle5_chain_manifest_filename_v1(0));
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
        assert_eq!(error.code_v1(), "cycle5_arm_v1_genesis_takes_no_panel");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn non_genesis_manifest_requires_its_panel_bytes_v1() {
        let dir = fresh_temp_dir_v1("panel-binding");
        let genesis = genesis_manifest_v1();
        write_chain_v1(&dir, &genesis);
        let panel =
            br#"{"schema":"mtg-kernel-cycle5-payoff-panel/v1","rank_sums":[0,0,0,0,0,0,0,0]}"#;
        let refresh_one = refresh_one_manifest_v1(&genesis, panel);
        let manifest_path = dir.join(cycle5_chain_manifest_filename_v1(1));
        std::fs::write(&manifest_path, refresh_one.canonical_bytes_v1()).expect("write");
        let panel_path = dir.join(cycle5_chain_panel_filename_v1(1));
        std::fs::write(&panel_path, panel).expect("write panel");

        let decoded =
            decode_interval_manifest_v1(&manifest_path, Some(&panel_path)).expect("decode");
        assert_eq!(decoded.refresh_index_v1(), 1);

        let missing = decode_interval_manifest_v1(&manifest_path, None)
            .expect_err("a non-genesis manifest needs its panel");
        assert_eq!(missing.code_v1(), "cycle5_arm_v1_missing_payoff_panel");

        let wrong_panel_path = dir.join("wrong.panel.json");
        std::fs::write(&wrong_panel_path, b"not the panel").expect("write");
        let mismatch = decode_interval_manifest_v1(&manifest_path, Some(&wrong_panel_path))
            .expect_err("panel content must resolve");
        assert_eq!(
            mismatch.code_v1(),
            "cycle5_arm_v1_refresh_manifest_rejected"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A genesis and a refresh-1 manifest bound to `run`'s own trainee
    /// identity, so the manifest/run binding gate passes and later gates are
    /// the ones under test.
    fn bound_manifests_v1(
        run: &ValidatedTrainRunV2,
    ) -> (Cycle5RefreshManifestV1, Cycle5RefreshManifestV1) {
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
    fn refresh_chained_arms_accept_an_advanced_manifest_v1() {
        for arm in [Cycle5ArmKindV1::ControlV3] {
            let run = run_for_arm_v1(arm);
            let (genesis, refresh_one) = bound_manifests_v1(&run);
            let at_genesis =
                validate_manifest_against_run_v1(&genesis, &run, arm).expect("genesis accepted");
            assert_eq!(at_genesis.program_update, 0);
            assert_eq!(
                at_genesis.trainee_local_generation,
                CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1
            );
            let at_one =
                validate_manifest_against_run_v1(&refresh_one, &run, arm).expect("refresh 1");
            assert_eq!(at_one.refresh_index, 1);
            assert_eq!(at_one.program_update, CYCLE5_REFRESH_INTERVAL_V1);
            assert_eq!(
                at_one.trainee_local_generation,
                CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1 + CYCLE5_REFRESH_INTERVAL_V1
            );
        }
    }

    #[test]
    fn manifest_must_bind_this_run_identity_v1() {
        let genesis = genesis_manifest_v1();
        let run = run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let error = validate_manifest_against_run_v1(&genesis, &run, Cycle5ArmKindV1::ControlV3)
            .expect_err("a manifest bound to another trainee must fail closed");
        assert_eq!(error.code_v1(), "cycle5_arm_v1_manifest_run_binding");
    }

    // ------------------------------------------------------------------
    // Interval stop
    // ------------------------------------------------------------------

    fn contract_at_v1(refresh_index: u64) -> Cycle5ArmContractV1 {
        let program_update = refresh_index * CYCLE5_REFRESH_INTERVAL_V1;
        Cycle5ArmContractV1 {
            refresh_index,
            program_update,
            trainee_local_generation: CYCLE5_TRAINEE_START_LOCAL_GENERATION_V1 + program_update,
        }
    }

    #[test]
    fn interval_stop_is_exactly_one_refresh_interval_v1() {
        let contract = contract_at_v1(2);
        validate_interval_stop_v1(384, 256, 4, &contract, Cycle5ArmKindV1::ControlV3, None)
            .expect("256 + 128 == 384");
        for stop in [383_u64, 385, 512] {
            assert_eq!(
                validate_interval_stop_v1(
                    stop,
                    256,
                    4,
                    &contract,
                    Cycle5ArmKindV1::ControlV3,
                    None
                )
                .expect_err("only one interval per process")
                .code_v1(),
                "cycle5_arm_v1_interval_stop"
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
                    Cycle5ArmKindV1::ControlV3,
                    None
                )
                .expect_err("not a whole refresh interval")
                .code_v1(),
                "cycle5_arm_v1_interval_stop"
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
        validate_interval_stop_v1(256, 256, 4, &contract, Cycle5ArmKindV1::ControlV3, None)
            .expect("refresh 1 opens 128 and stops at 256");
        // The ordinary start position for the same interval still passes.
        validate_interval_stop_v1(256, 128, 4, &contract, Cycle5ArmKindV1::ControlV3, None)
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
                Cycle5ArmKindV1::ControlV3,
                None
            )
            .expect_err("manifest 2 opens 256, not 128")
            .code_v1(),
            "cycle5_arm_v1_resume_position_mismatch"
        );
        // A position between the start and the stop is an interrupted
        // attempt resuming, which this same manifest and stop still cover.
        validate_interval_stop_v1(256, 192, 4, &contract, Cycle5ArmKindV1::ControlV3, None)
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
                Cycle5ArmKindV1::ControlV3,
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
                    Cycle5ArmKindV1::ControlV3,
                    None
                )
                .expect_err("not a checkpoint segment boundary")
                .code_v1(),
                "cycle5_arm_v1_interval_stop"
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
                    Cycle5ArmKindV1::ControlV3,
                    None
                )
                .expect_err("outside the interval this stop names")
                .code_v1(),
                "cycle5_arm_v1_interval_stop"
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
                Cycle5ArmKindV1::ControlV3,
                None
            )
            .expect_err("refresh 4 opens 512, not 384")
            .code_v1(),
            "cycle5_arm_v1_resume_position_mismatch"
        );
    }

    #[test]
    fn interval_stop_never_passes_the_program_end_v1() {
        let contract = contract_at_v1(CYCLE5_REFRESH_MAX_INDEX_V1);
        assert_eq!(
            validate_interval_stop_v1(2176, 2048, 4, &contract, Cycle5ArmKindV1::ControlV3, None)
                .expect_err("2048 is the program end")
                .code_v1(),
            "cycle5_arm_v1_interval_stop"
        );
        // The last TRAINED interval is the one refresh 15 opens (refresh 16
        // is the final panel boundary, not an interval start), and it is
        // admissible from either position.
        let final_interval = contract_at_v1(CYCLE5_REFRESH_MAX_INDEX_V1 - 1);
        validate_interval_stop_v1(
            2048,
            1920,
            4,
            &final_interval,
            Cycle5ArmKindV1::ControlV3,
            None,
        )
        .expect("the final interval ends exactly at the program end");
        validate_interval_stop_v1(
            2048,
            2048,
            4,
            &final_interval,
            Cycle5ArmKindV1::ControlV3,
            None,
        )
        .expect("a completed final interval resumes idempotently");
    }

    #[test]
    fn refresh_chained_arms_must_resume_at_the_manifest_position_v1() {
        let contract = contract_at_v1(2);
        assert_eq!(
            validate_interval_stop_v1(256, 128, 4, &contract, Cycle5ArmKindV1::ControlV3, None)
                .expect_err("manifest 2 opens generation 256, not 128")
                .code_v1(),
            "cycle5_arm_v1_resume_position_mismatch"
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
            Cycle5ArmKindV1::ControlV3,
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
                Cycle5ArmKindV1::ControlV3,
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
                    Cycle5ArmKindV1::ControlV3,
                    Some(4)
                )
                .expect_err("only resume + n")
                .code_v1(),
                "cycle5_arm_v1_interval_stop"
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
                    Cycle5ArmKindV1::ControlV3,
                    Some(updates)
                )
                .expect_err("outside 1..=8")
                .code_v1(),
                "cycle5_arm_v1_preflight_updates_range"
            );
        }
    }

    #[test]
    fn a_preflight_window_must_be_a_whole_number_of_checkpoint_segments_v1() {
        let contract = contract_at_v1(0);
        // Segment 4 cannot land exactly on a 2-update stop.
        assert_eq!(
            validate_interval_stop_v1(2, 0, 4, &contract, Cycle5ArmKindV1::ControlV3, Some(2))
                .expect_err("2 is not a multiple of 4")
                .code_v1(),
            "cycle5_arm_v1_interval_stop"
        );
        // A segment larger than the whole bound leaves no admissible window,
        // which is a legible rejection rather than a silent overshoot.
        assert_eq!(
            validate_interval_stop_v1(8, 0, 16, &contract, Cycle5ArmKindV1::ControlV3, Some(8))
                .expect_err("16 > 8")
                .code_v1(),
            "cycle5_arm_v1_interval_stop"
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
                Cycle5ArmKindV1::ControlV3,
                Some(8)
            )
            .expect_err("refresh 1 is not the genesis manifest")
            .code_v1(),
            "cycle5_arm_v1_preflight_manifest_advanced"
        );
        assert_eq!(
            validate_interval_stop_v1(
                136,
                128,
                4,
                &contract_at_v1(0),
                Cycle5ArmKindV1::ControlV3,
                Some(8)
            )
            .expect_err("136 is past the first refresh boundary")
            .code_v1(),
            "cycle5_arm_v1_preflight_manifest_advanced"
        );
        // Successive short windows inside the genesis interval are the
        // ladder's own shape and stay admissible.
        validate_interval_stop_v1(
            16,
            8,
            8,
            &contract_at_v1(0),
            Cycle5ArmKindV1::ControlV3,
            Some(8),
        )
        .expect("a second short window inside the genesis interval");
    }

    #[test]
    fn the_mode_marker_pins_a_store_prefix_to_one_training_mode_v1() {
        let root = fresh_temp_dir_v1("mode-marker");
        let run = run_for_arm_v1(Cycle5ArmKindV1::ControlV3);
        let prefix = root.join("prefix");

        // A preflight claims a fresh prefix, and re-claiming it in the same
        // mode is idempotent.
        claim_store_mode_marker_v1(
            &prefix,
            Cycle5ArmKindV1::ControlV3,
            &run,
            Cycle5ArmStoreModeV1::Preflight,
        )
        .expect("first claim");
        claim_store_mode_marker_v1(
            &prefix,
            Cycle5ArmKindV1::ControlV3,
            &run,
            Cycle5ArmStoreModeV1::Preflight,
        )
        .expect("same mode re-entry");
        // The formal path may not re-enter a prefix a preflight trained.
        assert_eq!(
            claim_store_mode_marker_v1(
                &prefix,
                Cycle5ArmKindV1::ControlV3,
                &run,
                Cycle5ArmStoreModeV1::Formal
            )
            .expect_err("formal may not adopt a preflight prefix")
            .code_v1(),
            "cycle5_arm_v1_mode_marker_conflict"
        );

        // And the reverse: a preflight is refused on a formal prefix.
        let formal = root.join("formal");
        claim_store_mode_marker_v1(
            &formal,
            Cycle5ArmKindV1::ControlV3,
            &run,
            Cycle5ArmStoreModeV1::Formal,
        )
        .expect("formal claim");
        assert_eq!(
            claim_store_mode_marker_v1(
                &formal,
                Cycle5ArmKindV1::ControlV3,
                &run,
                Cycle5ArmStoreModeV1::Preflight
            )
            .expect_err("a formal marker forbids the relaxed check")
            .code_v1(),
            "cycle5_arm_v1_mode_marker_conflict"
        );
        // A different arm on the same prefix is refused too.
        assert_eq!(
            claim_store_mode_marker_v1(
                &formal,
                Cycle5ArmKindV1::ControlV3,
                &run,
                Cycle5ArmStoreModeV1::Formal
            )
            .expect_err("one prefix, one arm")
            .code_v1(),
            "cycle5_arm_v1_mode_marker_conflict"
        );
        // A bootstrapped prefix may not be downgraded back to bootstrap.
        assert_eq!(
            claim_store_mode_marker_v1(
                &formal,
                Cycle5ArmKindV1::ControlV3,
                &run,
                Cycle5ArmStoreModeV1::Bootstrap
            )
            .expect_err("formal is terminal")
            .code_v1(),
            "cycle5_arm_v1_mode_marker_conflict"
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn a_bootstrapped_prefix_may_still_become_either_training_mode_v1() {
        let root = fresh_temp_dir_v1("mode-marker-bootstrap");
        let run = run_for_arm_v1(Cycle5ArmKindV1::ControlV3);

        // Nothing has trained after a bootstrap, so neither transition can
        // have been made under the relaxed interval check.
        for (name, mode) in [
            ("to-formal", Cycle5ArmStoreModeV1::Formal),
            ("to-preflight", Cycle5ArmStoreModeV1::Preflight),
        ] {
            let prefix = root.join(name);
            claim_store_mode_marker_v1(
                &prefix,
                Cycle5ArmKindV1::ControlV3,
                &run,
                Cycle5ArmStoreModeV1::Bootstrap,
            )
            .expect("bootstrap claim");
            claim_store_mode_marker_v1(&prefix, Cycle5ArmKindV1::ControlV3, &run, mode)
                .expect("a bootstrapped prefix admits either training mode");
            // ... but only once: the training mode is then terminal.
            let other = if matches!(mode, Cycle5ArmStoreModeV1::Formal) {
                Cycle5ArmStoreModeV1::Preflight
            } else {
                Cycle5ArmStoreModeV1::Formal
            };
            assert_eq!(
                claim_store_mode_marker_v1(&prefix, Cycle5ArmKindV1::ControlV3, &run, other)
                    .expect_err("the training mode is terminal")
                    .code_v1(),
                "cycle5_arm_v1_mode_marker_conflict"
            );
        }

        // A bootstrap for the wrong arm is refused before anything is opened.
        let prefix = root.join("wrong-arm");
        claim_store_mode_marker_v1(
            &prefix,
            Cycle5ArmKindV1::ControlV3,
            &run,
            Cycle5ArmStoreModeV1::Bootstrap,
        )
        .expect("bootstrap claim");
        assert_eq!(
            claim_store_mode_marker_v1(
                &prefix,
                Cycle5ArmKindV1::ControlV3,
                &run,
                Cycle5ArmStoreModeV1::Formal
            )
            .expect_err("the arm is fixed by the bootstrap")
            .code_v1(),
            "cycle5_arm_v1_mode_marker_conflict"
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn verifying_a_store_mode_marker_never_writes_one_v1() {
        let root = fresh_temp_dir_v1("mode-marker-verify");
        let arm = Cycle5ArmKindV1::ControlV3;
        let run = run_for_arm_v1(arm);
        let prefix = root.join("prefix");
        std::fs::create_dir_all(&prefix).expect("create prefix");
        let marker = prefix.join(CYCLE5_ARM_MODE_MARKER_FILENAME_V1);

        // An unclaimed prefix reports Absent and stays unclaimed.
        assert_eq!(
            verify_store_mode_marker_v1(&prefix, arm, &run, Cycle5ArmStoreModeV1::Formal)
                .expect("an unclaimed prefix admits any mode"),
            Cycle5ArmStoreModeStateV1::Absent
        );
        assert!(!marker.exists(), "verification never creates a marker");

        // A bootstrap marker reports promotable, and stays a bootstrap marker
        // until something actually claims it.
        claim_store_mode_marker_v1(&prefix, arm, &run, Cycle5ArmStoreModeV1::Bootstrap)
            .expect("bootstrap claim");
        let before = std::fs::read(&marker).expect("read marker");
        assert_eq!(
            verify_store_mode_marker_v1(&prefix, arm, &run, Cycle5ArmStoreModeV1::Formal)
                .expect("a bootstrap marker is promotable"),
            Cycle5ArmStoreModeStateV1::PromotableFromBootstrap
        );
        assert_eq!(
            std::fs::read(&marker).expect("read marker"),
            before,
            "verification never promotes on its own"
        );
        assert_eq!(
            verify_store_mode_marker_v1(&prefix, arm, &run, Cycle5ArmStoreModeV1::Bootstrap)
                .expect("the same mode is already claimed"),
            Cycle5ArmStoreModeStateV1::AlreadyClaimed
        );

        // A terminal mode is still refused by verification alone, so a caller
        // can fail closed before it opens anything.
        claim_store_mode_marker_v1(&prefix, arm, &run, Cycle5ArmStoreModeV1::Formal)
            .expect("promote to formal");
        assert_eq!(
            verify_store_mode_marker_v1(&prefix, arm, &run, Cycle5ArmStoreModeV1::Preflight)
                .expect_err("formal is terminal")
                .code_v1(),
            "cycle5_arm_v1_mode_marker_conflict"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// A rejected interval leaves the Store root exactly as it found it.
    ///
    /// `bootstrap_native_training_store_v2` creates the root, its lock and
    /// its directory skeleton, and clears a stale run-record staging file,
    /// so every check that can reject an invocation has to run BEFORE it.
    /// This drives the two that now do: the read-only genesis probe against
    /// an unseeded root, and the build-provenance gate against a root that
    /// looks seeded. In a unit test the running binary is never
    /// `cycle5_arm_v1.exe`, so the provenance gate always rejects, which is
    /// exactly the mismatched-launcher case.
    #[test]
    fn a_rejected_interval_never_creates_or_touches_the_store_root_v1() {
        let arm = Cycle5ArmKindV1::ControlV3;
        let root = fresh_temp_dir_v1("rejected-interval-no-side-effect");
        let base_seed = arm.formal_base_seed_v1();
        let run_record = root.join("run.json");
        let bytes = test_fixture_bytes_population_program_v2_cycle5_seeded_v1(arm.wire_v1());
        std::fs::write(&run_record, &bytes).expect("write run record");
        let run = decode_train_run_v2(&bytes).expect("fixture decodes");
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
        let store_root = prefix.join("store");
        let request = Cycle5ArmRequestV1 {
            arm,
            store_root: store_root.clone(),
            run_record,
            chain_dir: root.join("chain"),
            refresh_manifest: manifest_path,
            payoff_panel: None,
            slot_locator: locator_path,
            stop_generation: CYCLE5_REFRESH_INTERVAL_V1,
            preflight_updates: None,
        };

        // 1. An UNSEEDED root: the read-only genesis probe rejects, and the
        //    root is never created.
        let error = run_native_cycle5_arm_v1(&request).expect_err("an unseeded root is refused");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert!(
            !store_root.exists(),
            "a rejected interval must not create the Store root"
        );
        assert_eq!(
            std::fs::read_dir(&prefix).expect("list prefix").count(),
            0,
            "a rejected interval must leave the prefix empty"
        );

        // 2. A root that LOOKS seeded, so the probe passes and the
        //    build-provenance gate is what rejects. The root keeps exactly
        //    the one file planted in it: no lock, no skeleton, nothing.
        std::fs::create_dir_all(&store_root).expect("create store root");
        let latest = store_root.join(
            NativeTrainingStoreFinalNameV2::Latest
                .final_basename()
                .expect("latest leaf"),
        );
        std::fs::write(&latest, b"{}").expect("plant a latest pointer");
        let before = std::fs::read(&latest).expect("read planted latest");

        let error =
            run_native_cycle5_arm_v1(&request).expect_err("a mismatched launcher is refused");
        assert_eq!(error.code_v1(), "cycle5_arm_v1_build_provenance_mismatch");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(error.exit_code_v1(), 3);
        let entries: Vec<_> = std::fs::read_dir(&store_root)
            .expect("list store root")
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "a provenance rejection must not add anything to the root, saw {entries:?}"
        );
        assert_eq!(
            std::fs::read(&latest).expect("re-read latest"),
            before,
            "a provenance rejection must not rewrite what is already there"
        );
        assert!(
            !prefix.join(CYCLE5_ARM_MODE_MARKER_FILENAME_V1).exists(),
            "a provenance rejection must not claim the prefix"
        );

        std::fs::remove_dir_all(&root).expect("cleanup");
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
        let arm = Cycle5ArmKindV1::ControlV3;
        let run = seeded_run_for_arm_v1(arm);
        let base_seed = run.record().schedule().base_seed;

        // The three plain-file inputs an interval reads before it ever looks
        // at the Store.
        let run_record = root.join("run.json");
        std::fs::write(
            &run_record,
            test_fixture_bytes_population_program_v2_cycle5_seeded_v1(arm.wire_v1()),
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
        let marker = prefix.join(CYCLE5_ARM_MODE_MARKER_FILENAME_V1);
        let request = Cycle5ArmRequestV1 {
            arm,
            store_root: prefix.join("store"),
            run_record,
            chain_dir: root.join("chain"),
            refresh_manifest: manifest_path,
            payoff_panel: None,
            slot_locator: locator_path,
            stop_generation: CYCLE5_REFRESH_INTERVAL_V1,
            preflight_updates: None,
        };

        let error = run_native_cycle5_arm_v1(&request)
            .expect_err("an unseeded prefix has no interval to run");
        assert_eq!(error.code_v1(), "cycle5_arm_v1_genesis_not_bootstrapped");
        assert_eq!(error.failure_v1(), Cycle5ArmFailureV1::Contract);
        assert_eq!(error.exit_code_v1(), 3);
        assert!(
            !marker.exists(),
            "a rejected interval must not claim the prefix"
        );

        // The prefix is therefore still bootstrap-eligible, and the interval
        // mode can be promoted onto it afterwards exactly as it should be.
        claim_store_mode_marker_v1(&prefix, arm, &run, Cycle5ArmStoreModeV1::Bootstrap)
            .expect("a rejected interval leaves the prefix bootstrap-eligible");
        assert!(marker.is_file());
        claim_store_mode_marker_v1(&prefix, arm, &run, Cycle5ArmStoreModeV1::Formal)
            .expect("the bootstrapped prefix promotes to formal");

        // Repeating the interval is still refused, and now for the same
        // reason as before: the mode marker is not what rejects it.
        let stranded = root.join("stranded");
        claim_store_mode_marker_v1(&stranded, arm, &run, Cycle5ArmStoreModeV1::Formal)
            .expect("claim a formal prefix");
        assert_eq!(
            claim_store_mode_marker_v1(&stranded, arm, &run, Cycle5ArmStoreModeV1::Bootstrap)
                .expect_err("this is what the old ordering left behind")
                .code_v1(),
            "cycle5_arm_v1_mode_marker_conflict"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn store_root_parts_split_parent_and_basename_v1() {
        let (parent, basename) =
            store_root_parts_v1(Path::new("D:/cycle5/arm-a/store")).expect("split");
        assert_eq!(parent, PathBuf::from("D:/cycle5/arm-a"));
        assert_eq!(basename, "store");
    }

    // ------------------------------------------------------------------
    // Head-to-head boundary mode
    // ------------------------------------------------------------------

    #[test]
    fn the_h2h_boundary_mode_gate_admits_only_plain_v3_sides_v1() {
        assert_eq!(
            cycle5_h2h_boundary_mode_v1(false, None),
            Ok(Cycle5H2hBoundaryModeV1::Plain)
        );
        assert_eq!(
            cycle5_h2h_boundary_mode_v1(true, None),
            Err("cycle5_h2h_v5_run_unratified"),
            "a v5 side cannot be loaded by this build"
        );
        assert_eq!(
            cycle5_h2h_boundary_mode_v1(true, Some("D:/cycle5/arm-a/chain")),
            Err("cycle5_h2h_v5_run_unratified")
        );
        assert_eq!(
            cycle5_h2h_boundary_mode_v1(false, Some("D:/cycle5/arm-a/chain")),
            Err("cycle5_h2h_chain_dir_requires_v5_run"),
            "a chain directory on a v3 run would be silently ignored"
        );
    }
}
