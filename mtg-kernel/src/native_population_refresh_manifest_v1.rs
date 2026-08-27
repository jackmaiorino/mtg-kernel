//! Canonical refresh authority for the scaled Net8 population program.
//!
//! The manifest is external to StoreV2 checkpoint authority. It binds the
//! exact eight-policy snapshot installed for one completed 128-update
//! boundary. Later Store integration may carry its digest, but pool mutation
//! and checkpoint loading remain outside this pure module.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonErrorKindV1,
    CanonicalJsonErrorV1, CanonicalJsonNullPolicyV1,
};
use crate::kernel_native_search_opponent_v1::{
    KernelNativeSearchAuthorityV1, KernelNativeSearchTierV1, KERNEL_NATIVE_SEARCH_ALGORITHM_V1,
    KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1, KERNEL_NATIVE_SEARCH_AUTHORITY_SCHEMA_V1,
    KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1, KERNEL_NATIVE_SEARCH_EVALUATOR_IDENTITY_V1,
    KERNEL_NATIVE_SEARCH_NODE_KEY_V1, KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1,
};
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

pub(crate) const POPULATION_REFRESH_MANIFEST_SCHEMA_V1: &str =
    "mtg-kernel-scaled-selfplay-population-refresh/v1";
pub(crate) const POPULATION_PROGRAM_COMMIT_V1: &str = "838920e359c7a1152d97c450f4575c6be2309f22";
pub(crate) const POPULATION_PROGRAM_DOCUMENT_SHA256_V1: &str =
    "b0e836858379137e9f5068f1ed2d3cb98d0d6507d09170d8272caad2a989ea38";
pub(crate) const POPULATION_RETEST_MANIFEST_SHA256_V1: &str =
    "f3128e5f700830df2110d6abb06b5b6f7f8f642ac5064c5d3188afac93aed2c8";
pub(crate) const POPULATION_REPLAY_END_GENERATION_V1: u64 = 512;
pub(crate) const POPULATION_REFRESH_INTERVAL_V1: u64 = 128;
pub(crate) const POPULATION_BASE_REFRESH_COUNT_V1: u64 = 8;
pub(crate) const POPULATION_SLOT_COUNT_V1: usize = 8;
pub(crate) const POPULATION_WEIGHT_TOTAL_UNITS_V1: u64 = 1_000_000;
pub(crate) const POPULATION_ROLE_FLOOR_UNITS_V1: u64 = 200_000;
pub(crate) const POPULATION_POLICY_CAP_UNITS_V1: u64 = 250_000;
// CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md (countersigned 6a0db07d)
// Section 5 layer 3 / Section 7 / Section 9.2: the search authority-kind
// string is reused unchanged as the pool's third `occupant_class` value (no
// second identity minted); it is admitted only at slots 6/7
// (`POPULATION_SEARCH_SLOT_INDICES_V1`), only at tier T2048
// (`POPULATION_SEARCH_ENABLED_TIER_V1`; T8192/T512/T32768 fail closed exactly
// like an unregistered tier, gate 3), at most one such slot at a time, and
// capped at `POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1`, layered inside,
// never replacing, the general `POPULATION_POLICY_CAP_UNITS_V1` ceiling.
// AMENDMENT 2 (countersigned ab3d777b): this is the per-manifest IN-WINDOW
// maximum, not a cycle-average -- validate_slots_v1 is architecturally
// single-manifest (its only history-facing parameter, `previous`, feeds
// hash-chain/refresh-index continuity, never weight/economic bookkeeping),
// so a cycle-average cannot live in this check. The 2% cycle-wide average
// the registration sheet's own Section 9.2 originally derived this number
// from is instead owned and enforced arithmetically by the referencing
// campaign sheet's own fixed window set (cycle-3: 4 heavy refreshes at
// 80,000 + 12 refreshes at 0, `4*80,000/16 = 20,000` exactly, matching the
// original 2% figure on average while permitting an 80,000-unit peak in any
// single manifest). The five legacy Store-shaped hash fields on a
// search-occupied slot must equal `POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1`
// (never a real-looking hash), and `source_base_seed`/`source_generation`
// must be zero.
pub(crate) const POPULATION_SEARCH_SLOT_INDICES_V1: [usize; 2] = [6, 7];
pub(crate) const POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1: u64 = 80_000;
pub(crate) const POPULATION_SEARCH_ENABLED_TIER_V1: KernelNativeSearchTierV1 =
    KernelNativeSearchTierV1::T2048;
pub(crate) const POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
const POPULATION_LINEAGE_SEEDS_V1: [u64; 3] = [970_001, 970_002, 970_003];
const POPULATION_ANCHOR_BASE_SEEDS_V1: [u64; 2] = [920_012, 920_005];
const POPULATION_ANCHOR_GENERATIONS_V1: [u64; 2] = [384, 512];
const POPULATION_ANCHOR_RUN_SHA256S_V1: [&str; 2] = [
    "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae",
    "8bc06b6cf2e26df8002b5cece2784e0cd165cdd6bbd199a835e06c17e8d5de5c",
];
const POPULATION_ANCHOR_CHECKPOINT_SHA256S_V1: [&str; 2] = [
    "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8",
    "03f0e226f884f51bf7128f70bec189bd6ac2c8f231ced8886f2cb7d3e936cc90",
];
const POPULATION_ANCHOR_SIDECAR_SHA256S_V1: [&str; 2] = [
    "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb",
    "c56a8ba1361ab172c669307084c4522ee06ac79e39b7cf4a306f11effe36b031",
];
const POPULATION_ANCHOR_STATE_SHA256S_V1: [&str; 2] = [
    "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99",
    "2904dd7b899c21234c64925440277dbfa8d6f552d8f620b153bc8d16c44f523a",
];
const POPULATION_ANCHOR_MODEL_SHA256S_V1: [&str; 2] = [
    "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d",
    "0635d2defb8facd700ede34789434956fc4a2fd3b5058cc2df5dd820398b4c22",
];

const EXPECTED_ROLES_V1: [&str; POPULATION_SLOT_COUNT_V1] = [
    "anchor-0",
    "anchor-1",
    "historical-0",
    "historical-1",
    "current-0",
    "current-1",
    "exploiter-0",
    "exploiter-1",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PopulationRefreshSlotV1 {
    slot_index: u64,
    role: String,
    occupant_class: String,
    source_base_seed: u64,
    source_run_sha256: String,
    source_generation: u64,
    available_by_global_generation: u64,
    checkpoint_sha256: String,
    sidecar_sha256: String,
    state_sha256: String,
    model_parameter_sha256: String,
    weight_units: u64,
    // CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md Section 6 item 1. `Option`
    // with `skip_serializing_if` so every existing Store-only manifest
    // (this field always `None`) re-encodes byte-for-byte identical to
    // before this field existed; present only when `occupant_class` equals
    // `KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_authority: Option<PopulationSearchAuthoritySlotV1>,
}

/// The non-Store analog of a slot's five Store-identity hash fields: the
/// declared launch config for a `kernel-native-search-opponent-v1` pool
/// occupant. Mirrors `KernelNativeSearchAuthorityV1`'s own field set
/// (`kernel_native_search_opponent_v1.rs:85-101`) minus `schema` and
/// `authority_kind`, which are compile-bound constants reused here, not
/// manifest-declared data.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PopulationSearchAuthoritySlotV1 {
    tier: KernelNativeSearchTierV1,
    action_seed: u64,
    private_diagnostic_identity: String,
    evaluator_sha256: String,
    engine_commit: String,
    card_db_hash: u64,
    runtime_deck_catalog_sha256: String,
}

impl PopulationSearchAuthoritySlotV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_v1(
        tier: KernelNativeSearchTierV1,
        action_seed: u64,
        private_diagnostic_identity: impl Into<String>,
        evaluator_sha256: impl Into<String>,
        engine_commit: impl Into<String>,
        card_db_hash: u64,
        runtime_deck_catalog_sha256: impl Into<String>,
    ) -> Self {
        Self {
            tier,
            action_seed,
            private_diagnostic_identity: private_diagnostic_identity.into(),
            evaluator_sha256: evaluator_sha256.into(),
            engine_commit: engine_commit.into(),
            card_db_hash,
            runtime_deck_catalog_sha256: runtime_deck_catalog_sha256.into(),
        }
    }

    pub(crate) const fn tier_v1(&self) -> KernelNativeSearchTierV1 {
        self.tier
    }

    pub(crate) const fn action_seed_v1(&self) -> u64 {
        self.action_seed
    }

    /// Copies the live-build-derived fields
    /// (`evaluator_sha256`/`engine_commit`/`card_db_hash`/
    /// `runtime_deck_catalog_sha256`) out of an already-constructed,
    /// already-validated authority, so a manifest builder never
    /// hand-derives them a second way; `KernelNativeSearchAuthorityV1::current`
    /// is the one source of truth for what the live build produces.
    pub(crate) fn from_authority_v1(authority: &KernelNativeSearchAuthorityV1) -> Self {
        Self {
            tier: authority.tier,
            action_seed: authority.action_seed,
            private_diagnostic_identity: authority.private_diagnostic_identity.clone(),
            evaluator_sha256: authority.evaluator_sha256.clone(),
            engine_commit: authority.engine_commit.clone(),
            card_db_hash: authority.card_db_hash,
            runtime_deck_catalog_sha256: authority.runtime_deck_catalog_sha256.clone(),
        }
    }

    /// Reconstructs the full `KernelNativeSearchAuthorityV1` this slot
    /// declares, filling in the two compile-bound identity fields this
    /// wire type omits. Construction alone performs no validation; callers
    /// validate the result (Section 6 item 1: `.validate()` plus
    /// `matches_fresh_reconstruction_v1()`, two distinct checks).
    pub(crate) fn to_authority_v1(&self) -> KernelNativeSearchAuthorityV1 {
        KernelNativeSearchAuthorityV1 {
            schema: KERNEL_NATIVE_SEARCH_AUTHORITY_SCHEMA_V1.to_owned(),
            authority_kind: KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1.to_owned(),
            algorithm_identity: KERNEL_NATIVE_SEARCH_ALGORITHM_V1.to_owned(),
            node_key_identity: KERNEL_NATIVE_SEARCH_NODE_KEY_V1.to_owned(),
            tier: self.tier,
            transition_budget: self.tier.transition_budget(),
            policy_step_depth_cap: KERNEL_NATIVE_SEARCH_DEPTH_CAP_V1,
            seed_domain: KERNEL_NATIVE_SEARCH_SEED_DOMAIN_V1.to_owned(),
            evaluator_identity: KERNEL_NATIVE_SEARCH_EVALUATOR_IDENTITY_V1.to_owned(),
            evaluator_sha256: self.evaluator_sha256.clone(),
            engine_commit: self.engine_commit.clone(),
            card_db_hash: self.card_db_hash,
            runtime_deck_catalog_sha256: self.runtime_deck_catalog_sha256.clone(),
            private_diagnostic_identity: self.private_diagnostic_identity.clone(),
            action_seed: self.action_seed,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PopulationRefreshManifestWireV1 {
    schema: String,
    program_commit: String,
    program_document_sha256: String,
    retest_manifest_sha256: String,
    refresh_index: u64,
    program_update: u64,
    global_generation: u64,
    availability_generation: u64,
    weight_total_units: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_manifest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payoff_panel_sha256: Option<String>,
    slots: Vec<PopulationRefreshSlotV1>,
}

#[derive(Clone, Debug)]
pub(crate) struct PopulationRefreshManifestV1 {
    wire: PopulationRefreshManifestWireV1,
    canonical_bytes: Vec<u8>,
    manifest_sha256: [u8; 32],
}

impl PopulationRefreshManifestV1 {
    pub(crate) fn canonical_bytes_v1(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub(crate) const fn manifest_sha256_v1(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub(crate) const fn refresh_index_v1(&self) -> u64 {
        self.wire.refresh_index
    }

    pub(crate) const fn program_update_v1(&self) -> u64 {
        self.wire.program_update
    }

    pub(crate) const fn global_generation_v1(&self) -> u64 {
        self.wire.global_generation
    }

    pub(crate) fn slots_v1(&self) -> &[PopulationRefreshSlotV1] {
        &self.wire.slots
    }
}

impl PopulationRefreshSlotV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_v1(
        slot_index: u64,
        role: impl Into<String>,
        occupant_class: impl Into<String>,
        source_base_seed: u64,
        source_run_sha256: impl Into<String>,
        source_generation: u64,
        available_by_global_generation: u64,
        checkpoint_sha256: impl Into<String>,
        sidecar_sha256: impl Into<String>,
        state_sha256: impl Into<String>,
        model_parameter_sha256: impl Into<String>,
        weight_units: u64,
    ) -> Self {
        Self {
            slot_index,
            role: role.into(),
            occupant_class: occupant_class.into(),
            source_base_seed,
            source_run_sha256: source_run_sha256.into(),
            source_generation,
            available_by_global_generation,
            checkpoint_sha256: checkpoint_sha256.into(),
            sidecar_sha256: sidecar_sha256.into(),
            state_sha256: state_sha256.into(),
            model_parameter_sha256: model_parameter_sha256.into(),
            weight_units,
            search_authority: None,
        }
    }

    /// Builder-style: attaches a search-authority config to an
    /// otherwise-normal slot. Never changes `occupant_class`; the caller is
    /// responsible for passing the search authority-kind string as
    /// `occupant_class` to `new_v1` first. Kept separate from `new_v1`
    /// itself so no existing call site (Store-only slots) needs to change.
    pub(crate) fn with_search_authority_v1(
        mut self,
        search_authority: PopulationSearchAuthoritySlotV1,
    ) -> Self {
        self.search_authority = Some(search_authority);
        self
    }

    pub(crate) const fn slot_index_v1(&self) -> u64 {
        self.slot_index
    }

    pub(crate) fn role_v1(&self) -> &str {
        &self.role
    }

    pub(crate) fn occupant_class_v1(&self) -> &str {
        &self.occupant_class
    }

    pub(crate) fn search_authority_v1(&self) -> Option<&PopulationSearchAuthoritySlotV1> {
        self.search_authority.as_ref()
    }

    pub(crate) const fn weight_units_v1(&self) -> u64 {
        self.weight_units
    }

    pub(crate) const fn source_base_seed_v1(&self) -> u64 {
        self.source_base_seed
    }

    pub(crate) fn source_run_sha256_v1(&self) -> &str {
        &self.source_run_sha256
    }

    pub(crate) const fn source_generation_v1(&self) -> u64 {
        self.source_generation
    }

    pub(crate) fn checkpoint_sha256_v1(&self) -> &str {
        &self.checkpoint_sha256
    }

    pub(crate) fn sidecar_sha256_v1(&self) -> &str {
        &self.sidecar_sha256
    }

    pub(crate) fn state_sha256_v1(&self) -> &str {
        &self.state_sha256
    }

    pub(crate) fn model_parameter_sha256_v1(&self) -> &str {
        &self.model_parameter_sha256
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopulationRefreshManifestErrorKindV1 {
    CanonicalJson(CanonicalJsonErrorKindV1),
    InvalidAuthority,
    InvalidGeneration,
    InvalidChain,
    InvalidSlots,
    InvalidWeight,
    FutureCheckpoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PopulationRefreshManifestErrorV1 {
    kind: PopulationRefreshManifestErrorKindV1,
}

impl PopulationRefreshManifestErrorV1 {
    const fn new(kind: PopulationRefreshManifestErrorKindV1) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind_v1(self) -> PopulationRefreshManifestErrorKindV1 {
        self.kind
    }
}

impl From<CanonicalJsonErrorV1> for PopulationRefreshManifestErrorV1 {
    fn from(error: CanonicalJsonErrorV1) -> Self {
        Self::new(PopulationRefreshManifestErrorKindV1::CanonicalJson(
            error.kind(),
        ))
    }
}

impl Display for PopulationRefreshManifestErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.kind)
    }
}

impl Error for PopulationRefreshManifestErrorV1 {}

type Result<T> = std::result::Result<T, PopulationRefreshManifestErrorV1>;

pub(crate) fn build_population_refresh_manifest_v1(
    refresh_index: u64,
    previous: Option<&PopulationRefreshManifestV1>,
    payoff_panel_sha256: Option<&str>,
    slots: Vec<PopulationRefreshSlotV1>,
) -> Result<PopulationRefreshManifestV1> {
    let program_update = refresh_index
        .checked_mul(POPULATION_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidGeneration,
            )
        })?;
    let global_generation = POPULATION_REPLAY_END_GENERATION_V1
        .checked_add(program_update)
        .ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidGeneration,
            )
        })?;
    let wire = PopulationRefreshManifestWireV1 {
        schema: POPULATION_REFRESH_MANIFEST_SCHEMA_V1.to_owned(),
        program_commit: POPULATION_PROGRAM_COMMIT_V1.to_owned(),
        program_document_sha256: POPULATION_PROGRAM_DOCUMENT_SHA256_V1.to_owned(),
        retest_manifest_sha256: POPULATION_RETEST_MANIFEST_SHA256_V1.to_owned(),
        refresh_index,
        program_update,
        global_generation,
        availability_generation: global_generation,
        weight_total_units: POPULATION_WEIGHT_TOTAL_UNITS_V1,
        previous_manifest_sha256: previous
            .map(|manifest| lower_hex_raw32_v1(manifest.manifest_sha256_v1())),
        payoff_panel_sha256: payoff_panel_sha256.map(str::to_owned),
        slots,
    };
    let bytes = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    decode_population_refresh_manifest_v1(&bytes, previous)
}

pub(crate) fn decode_population_refresh_manifest_v1(
    bytes: &[u8],
    previous: Option<&PopulationRefreshManifestV1>,
) -> Result<PopulationRefreshManifestV1> {
    let wire: PopulationRefreshManifestWireV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid)?;
    let reencoded = to_canonical_json_bytes_v1(&wire, CanonicalJsonNullPolicyV1::Forbid)?;
    if reencoded != bytes {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidAuthority,
        ));
    }
    validate_wire_v1(&wire, previous)?;
    Ok(PopulationRefreshManifestV1 {
        manifest_sha256: sha256_v1(bytes),
        wire,
        canonical_bytes: bytes.to_vec(),
    })
}

fn validate_wire_v1(
    wire: &PopulationRefreshManifestWireV1,
    previous: Option<&PopulationRefreshManifestV1>,
) -> Result<()> {
    if wire.schema != POPULATION_REFRESH_MANIFEST_SCHEMA_V1
        || wire.program_commit != POPULATION_PROGRAM_COMMIT_V1
        || wire.program_document_sha256 != POPULATION_PROGRAM_DOCUMENT_SHA256_V1
        || wire.retest_manifest_sha256 != POPULATION_RETEST_MANIFEST_SHA256_V1
        || !is_sha256_v1(&wire.program_document_sha256)
        || !is_sha256_v1(&wire.retest_manifest_sha256)
    {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidAuthority,
        ));
    }
    let expected_program_update = wire
        .refresh_index
        .checked_mul(POPULATION_REFRESH_INTERVAL_V1)
        .ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidGeneration,
            )
        })?;
    let expected_global_generation = POPULATION_REPLAY_END_GENERATION_V1
        .checked_add(expected_program_update)
        .ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidGeneration,
            )
        })?;
    if wire.refresh_index > POPULATION_BASE_REFRESH_COUNT_V1
        || wire.program_update != expected_program_update
        || wire.global_generation != expected_global_generation
        || wire.availability_generation != expected_global_generation
    {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidGeneration,
        ));
    }
    match (wire.refresh_index, previous) {
        (0, None)
            if wire.previous_manifest_sha256.is_none() && wire.payoff_panel_sha256.is_none() => {}
        (0, _) => {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidChain,
            ));
        }
        (_, Some(previous)) => {
            let expected_previous = lower_hex_raw32_v1(previous.manifest_sha256_v1());
            if previous.refresh_index_v1().checked_add(1) != Some(wire.refresh_index)
                || wire.previous_manifest_sha256.as_deref() != Some(expected_previous.as_str())
                || wire
                    .payoff_panel_sha256
                    .as_deref()
                    .is_none_or(|value| !is_sha256_v1(value))
            {
                return Err(PopulationRefreshManifestErrorV1::new(
                    PopulationRefreshManifestErrorKindV1::InvalidChain,
                ));
            }
        }
        (_, None) => {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidChain,
            ));
        }
    }
    validate_slots_v1(wire)
}

fn validate_slots_v1(wire: &PopulationRefreshManifestWireV1) -> Result<()> {
    if wire.slots.len() != POPULATION_SLOT_COUNT_V1
        || wire.weight_total_units != POPULATION_WEIGHT_TOTAL_UNITS_V1
    {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidSlots,
        ));
    }
    let mut weight_sum = 0_u64;
    let mut model_hashes = std::collections::BTreeSet::new();
    let mut role_weights = [0_u64; 4];
    let mut search_occupied_slot_count = 0_u32;
    for (index, slot) in wire.slots.iter().enumerate() {
        let expected_index = u64::try_from(index).map_err(|_| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidSlots,
            )
        })?;
        let is_search_occupant = slot.occupant_class == KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1;
        let occupant_class_registered = matches!(
            slot.occupant_class.as_str(),
            "policy" | "historical-fallback"
        ) || is_search_occupant;
        // A search-occupied slot's five legacy Store-identity hashes must be
        // the fixed sentinel (never a real-looking hash); every other
        // occupant class keeps the ordinary is_sha256_v1 format check
        // unchanged (CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md Section 6
        // item 1).
        let legacy_hashes_valid = if is_search_occupant {
            slot.source_run_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
                && slot.checkpoint_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
                && slot.sidecar_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
                && slot.state_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
                && slot.model_parameter_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
        } else {
            is_sha256_v1(&slot.source_run_sha256)
                && is_sha256_v1(&slot.checkpoint_sha256)
                && is_sha256_v1(&slot.sidecar_sha256)
                && is_sha256_v1(&slot.state_sha256)
                && is_sha256_v1(&slot.model_parameter_sha256)
        };
        // search_authority is present if and only if occupant_class is the
        // search kind; never attached to a Store-backed slot, never absent
        // on a declared search slot.
        let search_authority_presence_valid =
            is_search_occupant == slot.search_authority_v1().is_some();
        if slot.slot_index != expected_index
            || slot.role != EXPECTED_ROLES_V1[index]
            || !occupant_class_registered
            || !legacy_hashes_valid
            || !search_authority_presence_valid
        {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidSlots,
            ));
        }
        validate_slot_assignment_v1(wire, index, slot)?;
        if slot.source_generation > slot.available_by_global_generation
            || slot.available_by_global_generation > wire.availability_generation
        {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::FutureCheckpoint,
            ));
        }
        let search_slot_weight_cap = if is_search_occupant {
            POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1
        } else {
            POPULATION_POLICY_CAP_UNITS_V1
        };
        if slot.weight_units == 0 || slot.weight_units > search_slot_weight_cap {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidWeight,
            ));
        }
        if is_search_occupant {
            search_occupied_slot_count += 1;
        }
        weight_sum = weight_sum.checked_add(slot.weight_units).ok_or_else(|| {
            PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidWeight,
            )
        })?;
        role_weights[index / 2] = role_weights[index / 2]
            .checked_add(slot.weight_units)
            .ok_or_else(|| {
                PopulationRefreshManifestErrorV1::new(
                    PopulationRefreshManifestErrorKindV1::InvalidWeight,
                )
            })?;
        // A search-occupied slot's model_parameter_sha256 is the shared
        // sentinel, not a real per-slot model hash; excluded from the
        // model-hash-uniqueness set so it never collides with itself or a
        // real hash (an all-zero sentinel is never a genuine model hash).
        if !is_search_occupant && !model_hashes.insert(slot.model_parameter_sha256.as_str()) {
            return Err(PopulationRefreshManifestErrorV1::new(
                PopulationRefreshManifestErrorKindV1::InvalidSlots,
            ));
        }
    }
    if weight_sum != POPULATION_WEIGHT_TOTAL_UNITS_V1
        || role_weights
            .iter()
            .any(|weight| *weight < POPULATION_ROLE_FLOOR_UNITS_V1)
        || search_occupied_slot_count > 1
    {
        return Err(PopulationRefreshManifestErrorV1::new(
            PopulationRefreshManifestErrorKindV1::InvalidWeight,
        ));
    }
    Ok(())
}

fn validate_slot_assignment_v1(
    wire: &PopulationRefreshManifestWireV1,
    index: usize,
    slot: &PopulationRefreshSlotV1,
) -> Result<()> {
    let invalid = || {
        PopulationRefreshManifestErrorV1::new(PopulationRefreshManifestErrorKindV1::InvalidSlots)
    };
    match index {
        0 | 1 => {
            if slot.source_base_seed != POPULATION_ANCHOR_BASE_SEEDS_V1[index]
                || slot.source_generation != POPULATION_ANCHOR_GENERATIONS_V1[index]
                || slot.source_run_sha256 != POPULATION_ANCHOR_RUN_SHA256S_V1[index]
                || slot.checkpoint_sha256 != POPULATION_ANCHOR_CHECKPOINT_SHA256S_V1[index]
                || slot.sidecar_sha256 != POPULATION_ANCHOR_SIDECAR_SHA256S_V1[index]
                || slot.state_sha256 != POPULATION_ANCHOR_STATE_SHA256S_V1[index]
                || slot.model_parameter_sha256 != POPULATION_ANCHOR_MODEL_SHA256S_V1[index]
                || slot.occupant_class != "policy"
            {
                return Err(invalid());
            }
        }
        2 | 3 => {
            let lineage_index =
                usize::try_from((wire.refresh_index + 2) % 3).map_err(|_| invalid())?;
            let lag = if index == 2 { 256 } else { 384 };
            if slot.source_base_seed != POPULATION_LINEAGE_SEEDS_V1[lineage_index]
                || slot.source_generation
                    != wire
                        .global_generation
                        .checked_sub(lag)
                        .ok_or_else(invalid)?
                || slot.occupant_class != "policy"
            {
                return Err(invalid());
            }
        }
        4 | 5 => {
            let offset = u64::try_from(index - 4).map_err(|_| invalid())?;
            let lineage_index =
                usize::try_from((wire.refresh_index + offset) % 3).map_err(|_| invalid())?;
            if slot.source_base_seed != POPULATION_LINEAGE_SEEDS_V1[lineage_index]
                || slot.source_generation != wire.global_generation
                || slot.occupant_class != "policy"
            {
                return Err(invalid());
            }
        }
        6 | 7 => {
            if slot.occupant_class == "policy" && slot.source_generation != 256 {
                return Err(invalid());
            }
            if slot.occupant_class == KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1 {
                validate_search_occupant_v1(
                    slot.source_base_seed,
                    slot.source_generation,
                    slot.search_authority.as_ref(),
                )?;
            }
        }
        _ => return Err(invalid()),
    }
    Ok(())
}

/// CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md Section 4/7/9.2/9.3: only
/// `T2048` is an enabled pool tier; `T8192` (reserved, gated on the Section
/// 13 concurrency measurement) and every other tier fail closed here exactly
/// like an unregistered tier, never silently accepted at an interpolated
/// weight. Two distinct authority checks (Section 6 item 1, mirroring
/// COUNTERSIGN amendment 3's "two distinct checks" pattern): field-by-field
/// `.validate()`, then the independent whole-record `matches_fresh_reconstruction_v1()`
/// rebuild-and-compare.
///
/// CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md Amendment 4 (countersigned
/// 00affe6a), A4.3(ii): generalized from `(slot: &PopulationRefreshSlotV1)`
/// to these three primitive parameters. The body reads exactly these three
/// fields and nothing else on the v1 slot type; generalizing the signature
/// (rather than adding a materially-identical sibling for
/// `PopulationTrancheRefreshSlotV2`, whose search-occupied slots carry the
/// same three fields under the same names/types) lets both the v1 and the
/// ported v2 tranche-refresh validator call the identical function. The one
/// existing v1 call site (`validate_slots_v1`, immediately above) is updated
/// to pass its slot's three fields directly; body and observable behavior
/// are otherwise unchanged.
fn validate_search_occupant_v1(
    source_base_seed: u64,
    source_generation: u64,
    search_authority: Option<&PopulationSearchAuthoritySlotV1>,
) -> Result<()> {
    let invalid = || {
        PopulationRefreshManifestErrorV1::new(PopulationRefreshManifestErrorKindV1::InvalidSlots)
    };
    if source_base_seed != 0 || source_generation != 0 {
        return Err(invalid());
    }
    let search_authority = search_authority.ok_or_else(invalid)?;
    if search_authority.tier_v1() != POPULATION_SEARCH_ENABLED_TIER_V1 {
        return Err(invalid());
    }
    let authority = search_authority.to_authority_v1();
    if authority.validate().is_err() || !authority.matches_fresh_reconstruction_v1() {
        return Err(invalid());
    }
    Ok(())
}

fn is_sha256_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

// ---------------------------------------------------------------------------
// Population program V2 tranche/cycle multiplicative-weights TRAINING
// refresh chain decoder. Ported from the cycle-2-era branch tip (commit
// `8c8d645`, main repo) per CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md
// Amendment 4 (countersigned `00affe6a`), Section A4.3. Additive versioned
// sibling: nothing above this line (v1's closed, population-v1-only chain)
// is edited except `validate_search_occupant_v1`'s signature (see its own
// updated doc comment, Amendment 4 A4.3(ii)). A THIRD, independent wire
// shape from both `PopulationRefreshManifestV1` above and the (deliberately
// NOT ported, per A4.3's own disclosed scope-narrowing) I9 pool-membership
// manifest: no `retest_manifest_sha256`, no `availability_generation`;
// `global_generation = refresh_index * POPULATION_TRANCHE_REFRESH_INTERVAL_V2`
// with no replay-import offset (tranche-1 starts this chain at generation 0;
// cycle-2 and cycle-3 are direct continuations of the SAME chain, not fresh
// ones -- Amendment 4 A4.2/A4.4).
//
// Field-shape correction (Amendment 4 A4.3(i), a port correction, not a new
// decision): field names match the real archived data
// (`checkpoint_manifest_sha256`, `checkpoint_payload_sha256`, `run_sha256`,
// `store_root`; no `sidecar_sha256`), confirmed against both `8c8d645`'s own
// struct and the archived `refresh-000.json`/`population-v2-refresh-003.json`.
//
// Searcher-occupant support (Amendment 4 A4.2's disclosed gap, A4.3(ii)):
// `8c8d645` predates the searcher-pool-registration work (PR #99/#106); its
// slot type carried no `occupant_class`/`search_authority` field at all.
// Added here, mirroring `PopulationRefreshSlotV1`'s identical shape and
// reusing `PopulationSearchAuthoritySlotV1` unchanged (slot-shape-agnostic)
// plus the five already-defined v1 search constants unchanged (none of them
// is v1-specific): `KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1`,
// `POPULATION_SEARCH_SLOT_INDICES_V1`, `POPULATION_SEARCH_ENABLED_TIER_V1`,
// `POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1`,
// `POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1`.
pub(crate) const POPULATION_TRANCHE_INITIAL_REFRESH_SCHEMA_V2: &str =
    "population-v2-tranche1-initial-refresh/v1";
pub(crate) const POPULATION_TRANCHE_REFRESH_SCHEMA_V2: &str = "population-v2-tranche1-refresh/v1";
pub(crate) const POPULATION_TRANCHE_REFRESH_INTERVAL_V2: u64 = 128;
// Amendment 4 A4.4: raised from `8c8d645`'s cycle-2 value (18) to 34, in
// place, additively, to accommodate cycle-3's own 16 further refreshes
// (refresh_index 19 through 34, global_generation 2,432 through 4,352 --
// exactly the parent's terminal 2,304 plus 128 per refresh, with no new
// offset term: the existing `refresh_index * 128` formula already produces
// this once the cap is raised, since cycle-2's own chain already anchors at
// generation 0 and cycle-3 is a direct continuation of the same chain).
pub(crate) const POPULATION_TRANCHE_REFRESH_MAX_INDEX_V2: u64 = 34;
const POPULATION_TRANCHE_REFRESH_SLOT_COUNT_V2: usize = 8;
const POPULATION_TRANCHE_REFRESH_FROZEN_SLOT_COUNT_V2: usize = 4;
pub(crate) const POPULATION_PACKAGE_COMMIT_V2: &str = "10ac4b7f24b6ff1fd7b40522b7a7a379b4f6f723";
pub(crate) const POPULATION_PROGRAM_DOCUMENT_SHA256_V2_PROPOSED: &str =
    "c3540f385cf2c8d7dae922deb3be10af913a006076077817cc61da109cfd6d88";
const POPULATION_TRANCHE_REFRESH_ROLE_FLOOR_UNITS_V2: u64 = POPULATION_ROLE_FLOOR_UNITS_V1;
const POPULATION_TRANCHE_REFRESH_POLICY_CAP_UNITS_V2: u64 = POPULATION_POLICY_CAP_UNITS_V1;
const POPULATION_TRANCHE_REFRESH_WEIGHT_TOTAL_UNITS_V2: u64 = POPULATION_WEIGHT_TOTAL_UNITS_V1;
// Search occupancy on a v2 tranche slot carries no real store_root (mirrors
// the legacy hash fields' own sentinel discipline); `store_root` is a
// path-shaped field, not a hash, so the sentinel is an empty string rather
// than a reuse of the 64-hex-char hash sentinel.
const POPULATION_SEARCH_SLOT_STORE_ROOT_SENTINEL_V2: &str = "";

fn population_tranche_default_occupant_class_v2() -> String {
    "policy".to_owned()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PopulationTrancheRefreshSlotV2 {
    slot_index: u64,
    role: String,
    // Amendment 4 A4.3(ii): absent on every real record predating the
    // searcher-pool work (the archived cycle-2 `population-v2-refresh-003.json`
    // among them, confirmed absent by direct inspection), so this defaults
    // to "policy" on decode -- the implicit meaning every pre-existing slot
    // already had -- rather than becoming a hard decode failure for
    // historical data this port must still accept (a field-shape
    // correction, not a new decision; see the module comment above).
    #[serde(default = "population_tranche_default_occupant_class_v2")]
    occupant_class: String,
    source_base_seed: u64,
    source_generation: u64,
    store_root: String,
    run_sha256: String,
    checkpoint_manifest_sha256: String,
    checkpoint_payload_sha256: String,
    model_parameter_sha256: String,
    weight_units: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    search_authority: Option<PopulationSearchAuthoritySlotV1>,
}

impl PopulationTrancheRefreshSlotV2 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_v2(
        slot_index: u64,
        role: impl Into<String>,
        occupant_class: impl Into<String>,
        source_base_seed: u64,
        source_generation: u64,
        store_root: impl Into<String>,
        run_sha256: impl Into<String>,
        checkpoint_manifest_sha256: impl Into<String>,
        checkpoint_payload_sha256: impl Into<String>,
        model_parameter_sha256: impl Into<String>,
        weight_units: u64,
    ) -> Self {
        Self {
            slot_index,
            role: role.into(),
            occupant_class: occupant_class.into(),
            source_base_seed,
            source_generation,
            store_root: store_root.into(),
            run_sha256: run_sha256.into(),
            checkpoint_manifest_sha256: checkpoint_manifest_sha256.into(),
            checkpoint_payload_sha256: checkpoint_payload_sha256.into(),
            model_parameter_sha256: model_parameter_sha256.into(),
            weight_units,
            search_authority: None,
        }
    }

    pub(crate) fn with_search_authority_v2(
        mut self,
        search_authority: PopulationSearchAuthoritySlotV1,
    ) -> Self {
        self.search_authority = Some(search_authority);
        self
    }

    pub(crate) const fn slot_index_v2(&self) -> u64 {
        self.slot_index
    }

    pub(crate) fn role_v2(&self) -> &str {
        &self.role
    }

    pub(crate) fn occupant_class_v2(&self) -> &str {
        &self.occupant_class
    }

    pub(crate) const fn source_base_seed_v2(&self) -> u64 {
        self.source_base_seed
    }

    pub(crate) const fn source_generation_v2(&self) -> u64 {
        self.source_generation
    }

    pub(crate) fn store_root_v2(&self) -> &str {
        &self.store_root
    }

    pub(crate) fn run_sha256_v2(&self) -> &str {
        &self.run_sha256
    }

    pub(crate) fn checkpoint_manifest_sha256_v2(&self) -> &str {
        &self.checkpoint_manifest_sha256
    }

    pub(crate) fn checkpoint_payload_sha256_v2(&self) -> &str {
        &self.checkpoint_payload_sha256
    }

    pub(crate) fn model_parameter_sha256_v2(&self) -> &str {
        &self.model_parameter_sha256
    }

    pub(crate) const fn weight_units_v2(&self) -> u64 {
        self.weight_units
    }

    pub(crate) fn search_authority_v2(&self) -> Option<&PopulationSearchAuthoritySlotV1> {
        self.search_authority.as_ref()
    }

    // Identity comparison excluding weight_units (which legitimately
    // changes every refresh cycle, frozen slots included): used only for
    // the frozen-slot (indices 0-3) immutability check across chain links.
    fn identity_matches_v2(&self, other: &Self) -> bool {
        self.slot_index == other.slot_index
            && self.role == other.role
            && self.occupant_class == other.occupant_class
            && self.source_base_seed == other.source_base_seed
            && self.source_generation == other.source_generation
            && self.store_root == other.store_root
            && self.run_sha256 == other.run_sha256
            && self.checkpoint_manifest_sha256 == other.checkpoint_manifest_sha256
            && self.checkpoint_payload_sha256 == other.checkpoint_payload_sha256
            && self.model_parameter_sha256 == other.model_parameter_sha256
            && self.search_authority == other.search_authority
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PopulationTrancheRefreshManifestWireV2 {
    schema: String,
    program_package_commit_v2: String,
    program_document_sha256_v2_proposed: String,
    refresh_index: u64,
    program_update: u64,
    global_generation: u64,
    weight_total_units: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_manifest_sha256: Option<String>,
    pool_manifest_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    payoff_panel_sha256: Option<String>,
    slots: Vec<PopulationTrancheRefreshSlotV2>,
}

#[derive(Clone, Debug)]
pub(crate) struct PopulationTrancheRefreshManifestV2 {
    wire: PopulationTrancheRefreshManifestWireV2,
    raw_bytes: Vec<u8>,
    manifest_sha256: [u8; 32],
}

impl PopulationTrancheRefreshManifestV2 {
    pub(crate) fn raw_bytes_v2(&self) -> &[u8] {
        &self.raw_bytes
    }

    pub(crate) const fn manifest_sha256_v2(&self) -> [u8; 32] {
        self.manifest_sha256
    }

    pub(crate) const fn refresh_index_v2(&self) -> u64 {
        self.wire.refresh_index
    }

    pub(crate) const fn program_update_v2(&self) -> u64 {
        self.wire.program_update
    }

    pub(crate) const fn global_generation_v2(&self) -> u64 {
        self.wire.global_generation
    }

    pub(crate) fn slots_v2(&self) -> &[PopulationTrancheRefreshSlotV2] {
        &self.wire.slots
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PopulationTrancheRefreshManifestErrorKindV2 {
    Json,
    InvalidAuthority,
    InvalidGeneration,
    InvalidChain,
    InvalidSlots,
    InvalidWeight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PopulationTrancheRefreshManifestErrorV2 {
    kind: PopulationTrancheRefreshManifestErrorKindV2,
}

impl PopulationTrancheRefreshManifestErrorV2 {
    const fn new(kind: PopulationTrancheRefreshManifestErrorKindV2) -> Self {
        Self { kind }
    }

    pub(crate) const fn kind_v2(self) -> PopulationTrancheRefreshManifestErrorKindV2 {
        self.kind
    }
}

impl Display for PopulationTrancheRefreshManifestErrorV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.kind)
    }
}

impl Error for PopulationTrancheRefreshManifestErrorV2 {}

type ResultTrancheV2<T> = std::result::Result<T, PopulationTrancheRefreshManifestErrorV2>;

/// Decodes and validates one link of the population-v2 tranche/cycle
/// multiplicative-weights training refresh chain (either the `-Phase
/// initial` seed link at `refresh_index` 0, or a later `-Phase refresh`
/// link). `previous` must be the immediately preceding link for every
/// `refresh_index` above 0, and must be `None` for `refresh_index` 0 --
/// mirroring `decode_population_refresh_manifest_v1`'s own chain contract.
/// Plain `serde_json` parsing, not canonical-JSON re-encode-and-compare
/// (cross-language rationale: both historical writers were PS1/Python, not
/// this crate's own canonical encoder; ported unchanged from `8c8d645`).
pub(crate) fn decode_population_tranche_refresh_manifest_v2(
    bytes: &[u8],
    previous: Option<&PopulationTrancheRefreshManifestV2>,
) -> ResultTrancheV2<PopulationTrancheRefreshManifestV2> {
    let wire: PopulationTrancheRefreshManifestWireV2 =
        serde_json::from_slice(bytes).map_err(|_| {
            PopulationTrancheRefreshManifestErrorV2::new(
                PopulationTrancheRefreshManifestErrorKindV2::Json,
            )
        })?;
    validate_tranche_refresh_wire_v2(&wire, previous)?;
    Ok(PopulationTrancheRefreshManifestV2 {
        manifest_sha256: sha256_v1(bytes),
        wire,
        raw_bytes: bytes.to_vec(),
    })
}

/// Schema/program-identity/generation-arithmetic checks only, no chain
/// continuity: split out of `validate_tranche_refresh_wire_v2` (which still
/// calls this, unchanged behavior) so
/// `decode_population_tranche_refresh_manifest_v2_current_only` (test-only;
/// see its own doc comment for why it exists) can reuse these checks
/// without the chain check that function cannot perform.
fn validate_tranche_refresh_authority_and_generation_v2(
    wire: &PopulationTrancheRefreshManifestWireV2,
) -> ResultTrancheV2<()> {
    let is_initial = wire.refresh_index == 0;
    let expected_schema = if is_initial {
        POPULATION_TRANCHE_INITIAL_REFRESH_SCHEMA_V2
    } else {
        POPULATION_TRANCHE_REFRESH_SCHEMA_V2
    };
    if wire.schema != expected_schema
        || wire.program_package_commit_v2 != POPULATION_PACKAGE_COMMIT_V2
        || wire.program_document_sha256_v2_proposed != POPULATION_PROGRAM_DOCUMENT_SHA256_V2_PROPOSED
        || wire.weight_total_units != POPULATION_TRANCHE_REFRESH_WEIGHT_TOTAL_UNITS_V2
        || !is_sha256_v1(&wire.pool_manifest_sha256)
    {
        return Err(PopulationTrancheRefreshManifestErrorV2::new(
            PopulationTrancheRefreshManifestErrorKindV2::InvalidAuthority,
        ));
    }
    let expected_generation = wire
        .refresh_index
        .checked_mul(POPULATION_TRANCHE_REFRESH_INTERVAL_V2)
        .ok_or_else(|| {
            PopulationTrancheRefreshManifestErrorV2::new(
                PopulationTrancheRefreshManifestErrorKindV2::InvalidGeneration,
            )
        })?;
    if wire.refresh_index > POPULATION_TRANCHE_REFRESH_MAX_INDEX_V2
        || wire.program_update != expected_generation
        || wire.global_generation != expected_generation
    {
        return Err(PopulationTrancheRefreshManifestErrorV2::new(
            PopulationTrancheRefreshManifestErrorKindV2::InvalidGeneration,
        ));
    }
    Ok(())
}

/// Evaluation-only sibling of [`decode_population_tranche_refresh_manifest_v2`]:
/// authenticates a single sealed link's schema, program-identity tags,
/// generation arithmetic, and eight-slot shape, but performs no chain-
/// continuity check against a predecessor (there is no `previous`
/// parameter). Exists for exactly one purpose: proving an archived REAL
/// historical link (e.g. the sealed cycle-2 `population-v2-refresh-003.json`
/// record, `refresh_index` 3) decodes correctly through the ported v2 path
/// on every check this crate can independently verify, when the full
/// historical chain back to `refresh_index` 0 is not available to this
/// worktree to reconstruct the `previous` argument the chain-checked
/// decoder would require. Mirrors the identical scope-narrowing already
/// established for the (deliberately not ported, Amendment 4 A4.3) I9
/// pool-membership manifest's own `decode_population_refresh_manifest_v2_current_only`.
#[cfg(test)]
pub(crate) fn decode_population_tranche_refresh_manifest_v2_current_only(
    bytes: &[u8],
) -> ResultTrancheV2<PopulationTrancheRefreshManifestV2> {
    let wire: PopulationTrancheRefreshManifestWireV2 =
        serde_json::from_slice(bytes).map_err(|_| {
            PopulationTrancheRefreshManifestErrorV2::new(
                PopulationTrancheRefreshManifestErrorKindV2::Json,
            )
        })?;
    validate_tranche_refresh_authority_and_generation_v2(&wire)?;
    validate_tranche_refresh_slots_v2(&wire, None)?;
    Ok(PopulationTrancheRefreshManifestV2 {
        manifest_sha256: sha256_v1(bytes),
        wire,
        raw_bytes: bytes.to_vec(),
    })
}

fn validate_tranche_refresh_wire_v2(
    wire: &PopulationTrancheRefreshManifestWireV2,
    previous: Option<&PopulationTrancheRefreshManifestV2>,
) -> ResultTrancheV2<()> {
    validate_tranche_refresh_authority_and_generation_v2(wire)?;
    let is_initial = wire.refresh_index == 0;
    match (is_initial, previous) {
        (true, None) => {
            if wire.previous_manifest_sha256.is_some() || wire.payoff_panel_sha256.is_some() {
                return Err(PopulationTrancheRefreshManifestErrorV2::new(
                    PopulationTrancheRefreshManifestErrorKindV2::InvalidChain,
                ));
            }
        }
        (true, Some(_)) | (false, None) => {
            return Err(PopulationTrancheRefreshManifestErrorV2::new(
                PopulationTrancheRefreshManifestErrorKindV2::InvalidChain,
            ));
        }
        (false, Some(previous)) => {
            let expected_previous = lower_hex_raw32_v1(previous.manifest_sha256_v2());
            if previous.refresh_index_v2().checked_add(1) != Some(wire.refresh_index)
                || wire.previous_manifest_sha256.as_deref() != Some(expected_previous.as_str())
                || wire
                    .payoff_panel_sha256
                    .as_deref()
                    .is_none_or(|value| !is_sha256_v1(value))
            {
                return Err(PopulationTrancheRefreshManifestErrorV2::new(
                    PopulationTrancheRefreshManifestErrorKindV2::InvalidChain,
                ));
            }
        }
    }
    validate_tranche_refresh_slots_v2(wire, previous)
}

fn validate_tranche_refresh_slots_v2(
    wire: &PopulationTrancheRefreshManifestWireV2,
    previous: Option<&PopulationTrancheRefreshManifestV2>,
) -> ResultTrancheV2<()> {
    let invalid_slots = || {
        PopulationTrancheRefreshManifestErrorV2::new(
            PopulationTrancheRefreshManifestErrorKindV2::InvalidSlots,
        )
    };
    let invalid_weight = || {
        PopulationTrancheRefreshManifestErrorV2::new(
            PopulationTrancheRefreshManifestErrorKindV2::InvalidWeight,
        )
    };
    if wire.slots.len() != POPULATION_TRANCHE_REFRESH_SLOT_COUNT_V2 {
        return Err(invalid_slots());
    }
    let mut weight_sum = 0_u64;
    let mut model_hashes = std::collections::BTreeSet::new();
    let mut role_weights = [0_u64; 4];
    let mut search_occupied_slot_count = 0_u32;
    for (index, slot) in wire.slots.iter().enumerate() {
        let expected_index = u64::try_from(index).map_err(|_| invalid_slots())?;
        let is_search_occupant = slot.occupant_class == KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1;
        // Amendment 4 A4.3(ii): search occupancy restricted to slots 6/7,
        // mirroring `validate_slot_assignment_v1`'s own structure exactly
        // (slots 0-5 hardcode `occupant_class == "policy"`; only 6/7 admit
        // "historical-fallback" or the search kind).
        let occupant_class_ok = if index < 6 {
            slot.occupant_class == "policy"
        } else {
            matches!(slot.occupant_class.as_str(), "policy" | "historical-fallback")
                || is_search_occupant
        };
        let legacy_fields_valid = if is_search_occupant {
            slot.run_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
                && slot.checkpoint_manifest_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
                && slot.checkpoint_payload_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
                && slot.model_parameter_sha256 == POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1
                && slot.store_root == POPULATION_SEARCH_SLOT_STORE_ROOT_SENTINEL_V2
        } else {
            is_sha256_v1(&slot.run_sha256)
                && is_sha256_v1(&slot.checkpoint_manifest_sha256)
                && is_sha256_v1(&slot.checkpoint_payload_sha256)
                && is_sha256_v1(&slot.model_parameter_sha256)
                && !slot.store_root.is_empty()
        };
        let search_authority_presence_valid = is_search_occupant == slot.search_authority.is_some();
        if slot.slot_index != expected_index
            || slot.role != EXPECTED_ROLES_V1[index]
            || !occupant_class_ok
            || !legacy_fields_valid
            || !search_authority_presence_valid
            || (is_search_occupant && !POPULATION_SEARCH_SLOT_INDICES_V1.contains(&index))
        {
            return Err(invalid_slots());
        }
        // Frozen-slot immutability (indices 0-3): must be byte-identical
        // (all identity fields except weight_units) to the immediately
        // preceding chain link, once one exists.
        if index < POPULATION_TRANCHE_REFRESH_FROZEN_SLOT_COUNT_V2 {
            if let Some(previous) = previous {
                if !slot.identity_matches_v2(&previous.slots_v2()[index]) {
                    return Err(invalid_slots());
                }
            }
        }
        if is_search_occupant {
            validate_search_occupant_v1(
                slot.source_base_seed,
                slot.source_generation,
                slot.search_authority.as_ref(),
            )
            .map_err(|_| invalid_slots())?;
            search_occupied_slot_count += 1;
        }
        if !is_search_occupant && !model_hashes.insert(slot.model_parameter_sha256.as_str()) {
            return Err(invalid_slots());
        }
        let search_slot_weight_cap = if is_search_occupant {
            POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1
        } else {
            POPULATION_TRANCHE_REFRESH_POLICY_CAP_UNITS_V2
        };
        if slot.weight_units == 0 || slot.weight_units > search_slot_weight_cap {
            return Err(invalid_weight());
        }
        weight_sum = weight_sum.checked_add(slot.weight_units).ok_or_else(invalid_weight)?;
        role_weights[index / 2] = role_weights[index / 2]
            .checked_add(slot.weight_units)
            .ok_or_else(invalid_weight)?;
    }
    if weight_sum != POPULATION_TRANCHE_REFRESH_WEIGHT_TOTAL_UNITS_V2
        || role_weights
            .iter()
            .any(|weight| *weight < POPULATION_TRANCHE_REFRESH_ROLE_FLOOR_UNITS_V2)
        || search_occupied_slot_count > 1
    {
        return Err(invalid_weight());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_native_search_opponent_v1::KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1;
    use serde_json::{json, Value};

    fn digest_v1(index: usize) -> String {
        format!("{index:064x}")
    }

    fn slots_v1(global_generation: u64) -> Vec<PopulationRefreshSlotV1> {
        let refresh_index = (global_generation - POPULATION_REPLAY_END_GENERATION_V1)
            / POPULATION_REFRESH_INTERVAL_V1;
        (0..POPULATION_SLOT_COUNT_V1)
            .map(|index| {
                let (
                    source_base_seed,
                    source_generation,
                    source_run,
                    checkpoint,
                    sidecar,
                    state,
                    model,
                ) = match index {
                    0 | 1 => (
                        POPULATION_ANCHOR_BASE_SEEDS_V1[index],
                        POPULATION_ANCHOR_GENERATIONS_V1[index],
                        POPULATION_ANCHOR_RUN_SHA256S_V1[index].to_owned(),
                        POPULATION_ANCHOR_CHECKPOINT_SHA256S_V1[index].to_owned(),
                        POPULATION_ANCHOR_SIDECAR_SHA256S_V1[index].to_owned(),
                        POPULATION_ANCHOR_STATE_SHA256S_V1[index].to_owned(),
                        POPULATION_ANCHOR_MODEL_SHA256S_V1[index].to_owned(),
                    ),
                    2 | 3 => {
                        let lineage =
                            POPULATION_LINEAGE_SEEDS_V1[((refresh_index + 2) % 3) as usize];
                        let lag = if index == 2 { 256 } else { 384 };
                        (
                            lineage,
                            global_generation - lag,
                            digest_v1(10 + index),
                            digest_v1(20 + index),
                            digest_v1(30 + index),
                            digest_v1(40 + index),
                            digest_v1(50 + index),
                        )
                    }
                    4 | 5 => {
                        let lineage = POPULATION_LINEAGE_SEEDS_V1
                            [((refresh_index + (index - 4) as u64) % 3) as usize];
                        (
                            lineage,
                            global_generation,
                            digest_v1(10 + index),
                            digest_v1(20 + index),
                            digest_v1(30 + index),
                            digest_v1(40 + index),
                            digest_v1(50 + index),
                        )
                    }
                    _ => (
                        980_000 + index as u64,
                        256,
                        digest_v1(10 + index),
                        digest_v1(20 + index),
                        digest_v1(30 + index),
                        digest_v1(40 + index),
                        digest_v1(50 + index),
                    ),
                };
                PopulationRefreshSlotV1::new_v1(
                    index as u64,
                    EXPECTED_ROLES_V1[index],
                    "policy",
                    source_base_seed,
                    source_run,
                    source_generation,
                    global_generation,
                    checkpoint,
                    sidecar,
                    state,
                    model,
                    125_000,
                )
            })
            .collect()
    }

    fn mutate_v1(
        manifest: &PopulationRefreshManifestV1,
        mutation: impl FnOnce(&mut Value),
        previous: Option<&PopulationRefreshManifestV1>,
    ) -> PopulationRefreshManifestErrorKindV1 {
        let mut value: Value = serde_json::from_slice(manifest.canonical_bytes_v1()).unwrap();
        mutation(&mut value);
        let bytes = to_canonical_json_bytes_v1(&value, CanonicalJsonNullPolicyV1::Forbid).unwrap();
        decode_population_refresh_manifest_v1(&bytes, previous)
            .unwrap_err()
            .kind_v1()
    }

    #[test]
    fn initial_and_chained_refresh_round_trip_canonically() {
        let initial = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        let next = build_population_refresh_manifest_v1(
            1,
            Some(&initial),
            Some(&digest_v1(90)),
            slots_v1(640),
        )
        .unwrap();
        assert_eq!(initial.program_update_v1(), 0);
        assert_eq!(initial.global_generation_v1(), 512);
        assert_eq!(next.program_update_v1(), 128);
        assert_eq!(next.global_generation_v1(), 640);
        assert_eq!(next.slots_v1().len(), 8);
        assert_eq!(next.slots_v1()[2].source_base_seed_v1(), 970_001);
        assert_eq!(next.slots_v1()[4].source_base_seed_v1(), 970_002);
        assert_eq!(next.slots_v1()[5].source_base_seed_v1(), 970_003);
        assert_eq!(
            decode_population_refresh_manifest_v1(next.canonical_bytes_v1(), Some(&initial))
                .unwrap()
                .canonical_bytes_v1(),
            next.canonical_bytes_v1()
        );
    }

    #[test]
    fn chain_and_generation_corruptions_fail_closed() {
        let initial = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        let next = build_population_refresh_manifest_v1(
            1,
            Some(&initial),
            Some(&digest_v1(90)),
            slots_v1(640),
        )
        .unwrap();
        assert_eq!(
            mutate_v1(
                &next,
                |value| value["refresh_index"] = json!(2),
                Some(&initial)
            ),
            PopulationRefreshManifestErrorKindV1::InvalidGeneration
        );
        assert_eq!(
            mutate_v1(
                &next,
                |value| value["previous_manifest_sha256"] = json!(digest_v1(91)),
                Some(&initial)
            ),
            PopulationRefreshManifestErrorKindV1::InvalidChain
        );
        assert_eq!(
            decode_population_refresh_manifest_v1(next.canonical_bytes_v1(), None)
                .unwrap_err()
                .kind_v1(),
            PopulationRefreshManifestErrorKindV1::InvalidChain
        );
    }

    #[test]
    fn slot_weight_duplicate_and_future_corruptions_fail_closed() {
        let initial = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][0]["weight_units"] = json!(250_001),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidWeight
        );
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][1]["model_parameter_sha256"] =
                    value["slots"][0]["model_parameter_sha256"].clone(),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][0]["available_by_global_generation"] = json!(513),
                None
            ),
            PopulationRefreshManifestErrorKindV1::FutureCheckpoint
        );
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][4]["source_base_seed"] = json!(970_003),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][2]["source_generation"] = json!(255),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["slots"][0]["checkpoint_sha256"] = json!(digest_v1(99)),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn unknown_missing_and_authority_corruptions_fail_closed() {
        let initial = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        assert!(matches!(
            mutate_v1(&initial, |value| value["unknown"] = json!(true), None),
            PopulationRefreshManifestErrorKindV1::CanonicalJson(_)
        ));
        assert_eq!(
            mutate_v1(
                &initial,
                |value| value["program_commit"] = json!("0".repeat(40)),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidAuthority
        );
        assert!(matches!(
            mutate_v1(
                &initial,
                |value| {
                    value.as_object_mut().unwrap().remove("slots");
                },
                None
            ),
            PopulationRefreshManifestErrorKindV1::CanonicalJson(_)
        ));
    }

    // CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md acceptance gates
    // (countersigned 6a0db07d), Section 10 gates 2 and 3: schema round-trip
    // and the fail-closed negative-test list.

    /// Slots 0-5 unchanged from `slots_v1`; slot 6 becomes search-occupied
    /// at the given tier/action_seed/weight, slot 7 absorbs the remaining
    /// weight of the (6,7) pair so the total and the pair floor both still
    /// hold (slots 0-5 sum to 750,000 of the 1,000,000 total, so 6+7 must
    /// sum to exactly 250,000).
    fn slots_with_search_at_6_v1(
        action_seed: u64,
        tier: KernelNativeSearchTierV1,
        search_weight: u64,
    ) -> Vec<PopulationRefreshSlotV1> {
        let mut slots = slots_v1(512);
        let authority = KernelNativeSearchAuthorityV1::current(
            tier,
            action_seed,
            valid_diagnostic_identity_v1(),
        )
        .expect("test authority must construct for a valid (tier, action_seed) pair");
        let search_authority = PopulationSearchAuthoritySlotV1::from_authority_v1(&authority);
        slots[6] = PopulationRefreshSlotV1::new_v1(
            6,
            EXPECTED_ROLES_V1[6],
            KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1,
            0,
            POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
            0,
            512,
            POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
            POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
            POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
            POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
            search_weight,
        )
        .with_search_authority_v1(search_authority);
        slots[7] = PopulationRefreshSlotV1::new_v1(
            7,
            EXPECTED_ROLES_V1[7],
            "policy",
            980_007,
            digest_v1(17),
            256,
            512,
            digest_v1(27),
            digest_v1(37),
            digest_v1(47),
            digest_v1(57),
            250_000 - search_weight,
        );
        slots
    }

    /// The diagnostic identity a `LegacyV1`-contract test run's real session
    /// would report is a different constant than a production population
    /// run's (`DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2`, Section 3);
    /// this manifest-schema test module never constructs a real session, so
    /// either of `KernelNativeSearchAuthorityV1::validate`'s two accepted
    /// values is equally valid here. Uses the environment-v2 one because
    /// that is what a real cycle-3 `population_program_v1` manifest would
    /// declare (Section 3).
    fn valid_diagnostic_identity_v1() -> &'static str {
        crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2
    }

    fn manifest_with_search_slot_v1(
        action_seed: u64,
        tier: KernelNativeSearchTierV1,
        search_weight: u64,
    ) -> PopulationRefreshManifestV1 {
        build_population_refresh_manifest_v1(
            0,
            None,
            None,
            slots_with_search_at_6_v1(action_seed, tier, search_weight),
        )
        .expect("a search slot at the enabled tier, authorized seed, and enabled cap must build")
    }

    #[test]
    fn search_occupied_slot_builds_and_round_trips_canonically() {
        let manifest = manifest_with_search_slot_v1(
            KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
            KernelNativeSearchTierV1::T2048,
            POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
        );
        // Schema round-trip (Section 10 gate 2): re-decoding the exact
        // canonical bytes and re-encoding again must reproduce them
        // byte-for-byte, and the search-occupied slot's fields survive
        // unchanged.
        let redecoded =
            decode_population_refresh_manifest_v1(manifest.canonical_bytes_v1(), None).unwrap();
        assert_eq!(
            redecoded.canonical_bytes_v1(),
            manifest.canonical_bytes_v1()
        );
        assert_eq!(
            redecoded.manifest_sha256_v1(),
            manifest.manifest_sha256_v1()
        );
        let slot = &redecoded.slots_v1()[6];
        assert_eq!(
            slot.occupant_class_v1(),
            KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1
        );
        assert_eq!(
            slot.weight_units_v1(),
            POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1
        );
        let search_authority = slot
            .search_authority_v1()
            .expect("search config must survive");
        assert_eq!(search_authority.tier_v1(), KernelNativeSearchTierV1::T2048);
        assert_eq!(
            search_authority.action_seed_v1(),
            KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0]
        );

        // An ordinary all-policy manifest is completely unaffected by the
        // new optional field's existence: it re-encodes identically to a
        // manifest built before this field existed (skip_serializing_if
        // omits it entirely when `None`).
        let ordinary = build_population_refresh_manifest_v1(0, None, None, slots_v1(512)).unwrap();
        assert!(
            !String::from_utf8_lossy(ordinary.canonical_bytes_v1()).contains("search_authority")
        );
    }

    #[test]
    fn unregistered_occupant_class_string_is_rejected() {
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| value["slots"][6]["occupant_class"] = json!("not-a-registered-kind"),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn t8192_declared_search_slot_is_hard_rejected() {
        // The coordinator's own named must-have gate: T8192 is a reserved
        // but NOT-enabled pool tier (Section 9.3); a slot declaring it fails
        // exactly like an unregistered tier, never accepted at some
        // interpolated weight.
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| value["slots"][6]["search_authority"]["tier"] = json!("t8192"),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn t512_and_t32768_declared_search_slots_are_also_rejected() {
        for tier in ["t512", "t32768"] {
            assert_eq!(
                mutate_v1(
                    &manifest_with_search_slot_v1(
                        KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                        KernelNativeSearchTierV1::T2048,
                        POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                    ),
                    |value| value["slots"][6]["search_authority"]["tier"] = json!(tier),
                    None
                ),
                PopulationRefreshManifestErrorKindV1::InvalidSlots,
                "tier {tier} must be rejected"
            );
        }
    }

    #[test]
    fn unauthorized_action_seed_is_rejected() {
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| value["slots"][6]["search_authority"]["action_seed"] = json!(1),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn tampered_authority_config_fails_fresh_reconstruction() {
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| {
                    value["slots"][6]["search_authority"]["evaluator_sha256"] =
                        json!("f".repeat(64))
                },
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn search_slot_at_a_non_exploiter_index_is_rejected() {
        // Index 4 (a "current" lineage slot) already requires
        // `occupant_class == "policy"`; declaring the search kind there
        // fails through that existing per-index rule, confirming search
        // occupancy never reaches an index outside {6, 7}.
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| value["slots"][4]["occupant_class"] =
                    json!(KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1),
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn both_exploiter_slots_search_occupied_is_rejected() {
        // At-most-one-search-slot (Section 7): forcing both 6 and 7 into
        // the search kind, even with an otherwise well-formed second
        // config, must fail (via the search-occupied-count check, the pair
        // floor, or both -- any rejection proves the at-most-one rule
        // holds).
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| {
                    value["slots"][7]["occupant_class"] =
                        json!(KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1);
                    value["slots"][7]["search_authority"] =
                        value["slots"][6]["search_authority"].clone();
                    for field in [
                        "source_run_sha256",
                        "checkpoint_sha256",
                        "sidecar_sha256",
                        "state_sha256",
                        "model_parameter_sha256",
                    ] {
                        value["slots"][7][field] = json!(POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1);
                    }
                    value["slots"][7]["source_base_seed"] = json!(0);
                    value["slots"][7]["source_generation"] = json!(0);
                },
                None
            ),
            // Rejected via InvalidWeight here specifically: slot 7 keeps its
            // original 230,000-unit weight, which now exceeds the
            // search-specific 20,000-unit cap on its own (a second,
            // independent reason beyond the at-most-one-search-slot rule
            // this test targets; both are enforced, this one simply runs
            // first in the per-slot loop).
            PopulationRefreshManifestErrorKindV1::InvalidWeight
        );
    }

    #[test]
    fn search_slot_weight_above_the_search_cap_is_rejected() {
        // 80,001 stays under the general 250,000-unit cap but exceeds the
        // search-specific 80,000-unit T2048 cap (Amendment 2, countersigned
        // ab3d777b: Section 7, Section 9.2, A2.3); slot 7 absorbs the
        // remainder (169,999) so the pair total (250,000) and the grand
        // total (1,000,000) both still hold, isolating the search-specific
        // cap from the general-cap and total-sum checks.
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| {
                    value["slots"][6]["weight_units"] = json!(80_001);
                    value["slots"][7]["weight_units"] = json!(169_999);
                },
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidWeight
        );
    }

    #[test]
    fn non_sentinel_legacy_hash_on_a_search_slot_is_rejected() {
        for field in [
            "source_run_sha256",
            "checkpoint_sha256",
            "sidecar_sha256",
            "state_sha256",
            "model_parameter_sha256",
        ] {
            assert_eq!(
                mutate_v1(
                    &manifest_with_search_slot_v1(
                        KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                        KernelNativeSearchTierV1::T2048,
                        POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                    ),
                    |value| value["slots"][6][field] = json!(digest_v1(999)),
                    None
                ),
                PopulationRefreshManifestErrorKindV1::InvalidSlots,
                "field {field} must reject a non-sentinel value on a search slot"
            );
        }
    }

    #[test]
    fn search_authority_on_a_policy_slot_is_rejected() {
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| {
                    value["slots"][0]["search_authority"] =
                        value["slots"][6]["search_authority"].clone();
                },
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
    }

    #[test]
    fn search_kind_slot_missing_search_authority_is_rejected() {
        assert_eq!(
            mutate_v1(
                &manifest_with_search_slot_v1(
                    KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                    KernelNativeSearchTierV1::T2048,
                    POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                ),
                |value| {
                    value["slots"][6]
                        .as_object_mut()
                        .unwrap()
                        .remove("search_authority");
                },
                None
            ),
            PopulationRefreshManifestErrorKindV1::InvalidSlots
        );
    }

    // -----------------------------------------------------------------
    // Population program V2 tranche/cycle refresh chain decoder tests.
    // CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md Amendment 4 (countersigned
    // 00affe6a), A4.5 acceptance gates.
    // -----------------------------------------------------------------

    mod tranche_refresh_v2 {
        use super::*;

        fn tranche_digest(seed: usize) -> String {
            format!("{seed:064x}")
        }

        const TRANCHE_ROLES_V2: [&str; 8] = [
            "anchor-0", "anchor-1", "historical-0", "historical-1", "current-0", "current-1",
            "exploiter-0", "exploiter-1",
        ];
        const TRANCHE_SEEDS_V2: [u64; 8] = [
            920_012, 970_002, 971_221, 971_223, 972_001, 972_002, 971_231, 971_233,
        ];

        // `source_generation` is deliberately FIXED (not tied to the
        // manifest's own `global_generation`): unlike v1's mechanism, the
        // v2 tranche validator never requires a slot's `source_generation`
        // to equal `global_generation` (Amendment 4 A4.2: "no requirement
        // that source_generation equal global_generation" for this
        // mechanism). Keeping it fixed also keeps frozen slots 0-3
        // trivially identity-stable across chain links in these fixtures.
        fn tranche_slot_v2(index: usize, hash_seed: usize) -> Value {
            json!({
                "slot_index": index as u64,
                "role": TRANCHE_ROLES_V2[index],
                "occupant_class": "policy",
                "source_base_seed": TRANCHE_SEEDS_V2[index],
                "source_generation": 384_u64,
                "store_root": format!("D:\\fixture\\tranche-v2\\slot-{index}"),
                "run_sha256": tranche_digest(10 + hash_seed),
                "checkpoint_manifest_sha256": tranche_digest(20 + hash_seed),
                "checkpoint_payload_sha256": tranche_digest(30 + hash_seed),
                "model_parameter_sha256": tranche_digest(40 + hash_seed),
                "weight_units": 125_000_u64,
            })
        }

        fn tranche_manifest_v2(
            refresh_index: u64,
            global_generation: u64,
            previous_manifest_sha256: Option<String>,
            payoff_panel_sha256: Option<String>,
            slot_override: impl FnOnce(&mut Vec<Value>),
        ) -> Vec<u8> {
            let mut slots: Vec<Value> = (0..8)
                .map(|index| tranche_slot_v2(index, 100 + index))
                .collect();
            slot_override(&mut slots);
            let schema = if refresh_index == 0 {
                POPULATION_TRANCHE_INITIAL_REFRESH_SCHEMA_V2
            } else {
                POPULATION_TRANCHE_REFRESH_SCHEMA_V2
            };
            let wire = json!({
                "schema": schema,
                "program_package_commit_v2": POPULATION_PACKAGE_COMMIT_V2,
                "program_document_sha256_v2_proposed": POPULATION_PROGRAM_DOCUMENT_SHA256_V2_PROPOSED,
                "refresh_index": refresh_index,
                "program_update": global_generation,
                "global_generation": global_generation,
                "weight_total_units": 1_000_000_u64,
                "previous_manifest_sha256": previous_manifest_sha256,
                "pool_manifest_sha256": tranche_digest(1),
                "payoff_panel_sha256": payoff_panel_sha256,
                "slots": slots,
            });
            serde_json::to_vec(&wire).unwrap()
        }

        fn initial_bytes() -> Vec<u8> {
            tranche_manifest_v2(0, 0, None, None, |_| {})
        }

        #[test]
        fn initial_link_round_trips() {
            let bytes = initial_bytes();
            let decoded = decode_population_tranche_refresh_manifest_v2(&bytes, None).unwrap();
            assert_eq!(decoded.refresh_index_v2(), 0);
            assert_eq!(decoded.global_generation_v2(), 0);
            assert_eq!(decoded.slots_v2().len(), 8);
            assert_eq!(decoded.slots_v2()[4].role_v2(), "current-0");
        }

        #[test]
        fn second_link_chains_onto_the_first() {
            let initial = decode_population_tranche_refresh_manifest_v2(&initial_bytes(), None).unwrap();
            let previous_sha = lower_hex_raw32_v1(initial.manifest_sha256_v2());
            let next_bytes = tranche_manifest_v2(1, 128, Some(previous_sha), Some(tranche_digest(2)), |_| {});
            let next = decode_population_tranche_refresh_manifest_v2(&next_bytes, Some(&initial)).unwrap();
            assert_eq!(next.refresh_index_v2(), 1);
            assert_eq!(next.global_generation_v2(), 128);
        }

        /// A4.5: "refresh 17 rejects" -- cycle-3's own 17th refresh, one past
        /// its 16 authorized ones, is chain refresh_index 35, one past the
        /// raised cap of 34.
        #[test]
        fn refresh_index_35_rejects_one_past_the_raised_cap() {
            let bytes = tranche_manifest_v2(35, 35 * 128, None, None, |_| {});
            assert_eq!(
                decode_population_tranche_refresh_manifest_v2(&bytes, None)
                    .unwrap_err()
                    .kind_v2(),
                PopulationTrancheRefreshManifestErrorKindV2::InvalidGeneration
            );
        }

        /// The new cap itself (34) is still accepted, confirming the raise
        /// is a widen, not an off-by-one: refresh_index=34, decoded stand-
        /// alone via the evaluation-only path (no full 34-link chain built
        /// in this unit test), still passes every non-chain check.
        #[test]
        fn refresh_index_34_the_new_cap_itself_is_accepted() {
            let bytes = tranche_manifest_v2(34, 34 * 128, Some(tranche_digest(9)), Some(tranche_digest(2)), |_| {});
            assert!(decode_population_tranche_refresh_manifest_v2_current_only(&bytes).is_ok());
        }

        #[test]
        fn wrong_generation_arithmetic_rejects() {
            // global_generation must equal refresh_index * 128 exactly;
            // here refresh_index=3 (expected 384) but global_generation is
            // declared as 383.
            let slots: Vec<Value> = (0..8).map(|index| tranche_slot_v2(index, 100 + index)).collect();
            let wire = json!({
                "schema": POPULATION_TRANCHE_REFRESH_SCHEMA_V2,
                "program_package_commit_v2": POPULATION_PACKAGE_COMMIT_V2,
                "program_document_sha256_v2_proposed": POPULATION_PROGRAM_DOCUMENT_SHA256_V2_PROPOSED,
                "refresh_index": 3_u64,
                "program_update": 383_u64,
                "global_generation": 383_u64,
                "weight_total_units": 1_000_000_u64,
                "previous_manifest_sha256": Value::Null,
                "pool_manifest_sha256": tranche_digest(1),
                "payoff_panel_sha256": Value::Null,
                "slots": slots,
            });
            let bytes = serde_json::to_vec(&wire).unwrap();
            assert_eq!(
                decode_population_tranche_refresh_manifest_v2_current_only(&bytes)
                    .unwrap_err()
                    .kind_v2(),
                PopulationTrancheRefreshManifestErrorKindV2::InvalidGeneration
            );
        }

        /// Builds the search-authority sub-record the SAME way the v1
        /// mechanism's own `slots_with_search_at_6_v1` fixture does
        /// (`KernelNativeSearchAuthorityV1::current` + `from_authority_v1`),
        /// rather than hand-constructing its live-build-derived fields
        /// (`evaluator_sha256`/`engine_commit`/`card_db_hash`/
        /// `runtime_deck_catalog_sha256`), which would risk not matching
        /// what `matches_fresh_reconstruction_v1()` independently rebuilds.
        fn search_occupied_slot_v2(action_seed: u64, tier: KernelNativeSearchTierV1, weight_units: u64) -> Value {
            let authority = KernelNativeSearchAuthorityV1::current(
                tier,
                action_seed,
                valid_diagnostic_identity_v1(),
            )
            .expect("test authority must construct for a valid (tier, action_seed) pair");
            let search_authority = PopulationSearchAuthoritySlotV1::from_authority_v1(&authority);
            json!({
                "slot_index": 6_u64,
                "role": "exploiter-0",
                "occupant_class": KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1,
                "source_base_seed": 0_u64,
                "source_generation": 0_u64,
                "store_root": "",
                "run_sha256": POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
                "checkpoint_manifest_sha256": POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
                "checkpoint_payload_sha256": POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
                "model_parameter_sha256": POPULATION_SEARCH_SLOT_SENTINEL_HASH_V1,
                "weight_units": weight_units,
                "search_authority": serde_json::to_value(&search_authority).unwrap(),
            })
        }

        // Rebalances slot 7 to absorb whatever slot 6 gives up, keeping the
        // 1,000,000-unit total exact regardless of the search slot's own
        // weight (each `tranche_slot_v2` default is 125,000; slot 7's own
        // weight becomes `125_000 + (125_000 - weight_units)` so the
        // 6/7 pair, and the grand total, stay correct even when
        // `weight_units` is deliberately invalid for a negative test).
        fn manifest_with_search_slot_v2(action_seed: u64, tier: KernelNativeSearchTierV1, weight_units: u64) -> Vec<u8> {
            tranche_manifest_v2(0, 0, None, None, |slots| {
                slots[6] = search_occupied_slot_v2(action_seed, tier, weight_units);
                slots[7]["weight_units"] = json!(250_000_u64.saturating_sub(weight_units));
            })
        }

        #[test]
        fn search_slot_at_t2048_80000_units_round_trips() {
            let bytes = manifest_with_search_slot_v2(
                KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                KernelNativeSearchTierV1::T2048,
                POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
            );
            let decoded = decode_population_tranche_refresh_manifest_v2(&bytes, None).unwrap();
            assert!(decoded.slots_v2()[6].search_authority_v2().is_some());
            assert_eq!(decoded.slots_v2()[6].weight_units_v2(), 80_000);
        }

        #[test]
        fn search_slot_wrong_tier_rejects() {
            let bytes = manifest_with_search_slot_v2(
                KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                KernelNativeSearchTierV1::T8192,
                POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
            );
            assert_eq!(
                decode_population_tranche_refresh_manifest_v2(&bytes, None)
                    .unwrap_err()
                    .kind_v2(),
                PopulationTrancheRefreshManifestErrorKindV2::InvalidSlots
            );
        }

        #[test]
        fn search_slot_weight_above_the_search_cap_is_rejected() {
            let bytes = manifest_with_search_slot_v2(
                KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                KernelNativeSearchTierV1::T2048,
                POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1 + 1,
            );
            assert_eq!(
                decode_population_tranche_refresh_manifest_v2(&bytes, None)
                    .unwrap_err()
                    .kind_v2(),
                PopulationTrancheRefreshManifestErrorKindV2::InvalidWeight
            );
        }

        #[test]
        fn search_slot_unauthorized_action_seed_is_rejected() {
            // `KernelNativeSearchAuthorityV1::current` itself refuses to
            // construct for an unauthorized seed (mirrors v1's own
            // `unauthorized_action_seed_is_rejected` test): build a
            // genuinely valid manifest first, then mutate the ALREADY-
            // ENCODED bytes' `action_seed` field afterward, bypassing the
            // constructor's own validation, exactly as v1's test does.
            let bytes = manifest_with_search_slot_v2(
                KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                KernelNativeSearchTierV1::T2048,
                POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
            );
            let mut value: Value = serde_json::from_slice(&bytes).unwrap();
            value["slots"][6]["search_authority"]["action_seed"] =
                json!(KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0] + 1);
            let tampered = serde_json::to_vec(&value).unwrap();
            assert_eq!(
                decode_population_tranche_refresh_manifest_v2(&tampered, None)
                    .unwrap_err()
                    .kind_v2(),
                PopulationTrancheRefreshManifestErrorKindV2::InvalidSlots
            );
        }

        #[test]
        fn search_slot_at_a_non_exploiter_index_is_rejected() {
            let bytes = tranche_manifest_v2(0, 0, None, None, |slots| {
                slots[4] = {
                    let mut slot = search_occupied_slot_v2(
                        KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1[0],
                        KernelNativeSearchTierV1::T2048,
                        POPULATION_SEARCH_SLOT_T2048_WEIGHT_UNITS_V1,
                    );
                    slot["slot_index"] = json!(4_u64);
                    slot["role"] = json!("current-0");
                    slot
                };
            });
            assert_eq!(
                decode_population_tranche_refresh_manifest_v2(&bytes, None)
                    .unwrap_err()
                    .kind_v2(),
                PopulationTrancheRefreshManifestErrorKindV2::InvalidSlots
            );
        }

        #[test]
        fn frozen_slot_mutation_between_links_is_rejected() {
            let initial = decode_population_tranche_refresh_manifest_v2(&initial_bytes(), None).unwrap();
            let previous_sha = lower_hex_raw32_v1(initial.manifest_sha256_v2());
            // Slot 0 (anchor-0, frozen) changes its model_parameter_sha256
            // between links -- structurally identical to how the frozen
            // slots must never move once sealed.
            let next_bytes = tranche_manifest_v2(
                1,
                128,
                Some(previous_sha),
                Some(tranche_digest(2)),
                |slots| {
                    slots[0]["model_parameter_sha256"] = json!(tranche_digest(999));
                },
            );
            assert_eq!(
                decode_population_tranche_refresh_manifest_v2(&next_bytes, Some(&initial))
                    .unwrap_err()
                    .kind_v2(),
                PopulationTrancheRefreshManifestErrorKindV2::InvalidSlots
            );
        }

        /// A4.5: "v1 manifests still decode" -- population-v1's own real,
        /// archived, byte-exact wire manifest (its OWN closed 8-refresh
        /// chain, entirely unrelated to and unaffected by this port) still
        /// decodes through the untouched `decode_population_refresh_manifest_v1`
        /// path. Read-only against a real file on this host, not a fixture;
        /// skipped (not failed) if that file is unavailable on the host
        /// running this test, matching this crate's own established
        /// pattern for real-external-file regression reads elsewhere in
        /// this file's sibling test modules.
        #[test]
        fn v1_archived_population_v1_refresh_000_still_decodes() {
            let path = r"D:\mtg-kernel-scaled-selfplay-population-v1\active\initial-refresh\refresh-000.json";
            let Ok(bytes) = std::fs::read(path) else {
                eprintln!("skipping: {path} not present on this host");
                return;
            };
            // The archive copy carries a trailing CRLF (a filesystem/editor
            // artifact from whatever tool last touched the file on disk,
            // confirmed by direct byte inspection: the content itself is
            // already canonical, sorted-key, single-line JSON); this
            // crate's own canonical-JSON contract requires exactly one
            // trailing LF (confirmed: stripping it outright, rather than
            // normalizing to exactly one LF, trips `MissingFinalLf`), so
            // the CRLF is normalized to a single LF here, rather than
            // treated as a real decode failure, since it is not part of
            // the sealed content itself.
            let mut normalized = bytes.trim_ascii_end().to_vec();
            normalized.push(b'\n');
            let decoded = decode_population_refresh_manifest_v1(&normalized, None)
                .expect("population-v1's own real refresh-000.json must still decode unchanged");
            assert_eq!(decoded.refresh_index_v1(), 0);
            assert_eq!(decoded.global_generation_v1(), 512);
        }

        /// A4.5: "the archived cycle-2 refresh-003 through the ported v2
        /// path... must decode." Read-only against the real archived file;
        /// decoded via the evaluation-only path (see
        /// `decode_population_tranche_refresh_manifest_v2_current_only`'s own
        /// doc comment for why: this worktree does not hold the full
        /// historical chain back to refresh_index 0 needed to construct a
        /// `previous` argument for the chain-checked decoder). Skipped, not
        /// failed, if the archive is unavailable on the host running this
        /// test.
        #[test]
        fn archived_cycle2_refresh_003_decodes_through_the_ported_v2_path() {
            let path = r"E:\c-evidence-archive-20260825\mtg-kernel-population-v2-cycle2\refresh\cycle2-population-v2-refresh-0384\attempt-001\population-v2-refresh-003.json";
            let Ok(bytes) = std::fs::read(path) else {
                eprintln!("skipping: {path} not present on this host");
                return;
            };
            let decoded = decode_population_tranche_refresh_manifest_v2_current_only(&bytes)
                .expect("the archived cycle-2 refresh-003 record must decode through the ported v2 path");
            assert_eq!(decoded.refresh_index_v2(), 3);
            assert_eq!(decoded.global_generation_v2(), 384);
            assert_eq!(decoded.slots_v2()[4].role_v2(), "current-0");
            assert_eq!(decoded.slots_v2()[4].source_base_seed_v2(), 975_001);
        }

        /// Task 6 manifest-authoring aid, not a correctness gate: prints the
        /// real, live-build search-authority JSON sub-record (T2048, the
        /// authorized production seed 2026082601) to stdout, for splicing
        /// into hand/script-authored cycle-3 refresh manifests at slots 6/7
        /// during the four heavy windows. `evaluator_sha256`/`engine_commit`/
        /// `card_db_hash`/`runtime_deck_catalog_sha256` are live-build-derived
        /// and must come from this exact construction path
        /// (`KernelNativeSearchAuthorityV1::current`), never hand-guessed,
        /// or `matches_fresh_reconstruction_v1()` at decode/resolution time
        /// will reject the slot.
        #[test]
        #[ignore = "manifest-authoring aid: run explicitly with --nocapture to print the real search-authority JSON"]
        fn print_real_search_authority_json_for_manifest_authoring() {
            let authority = KernelNativeSearchAuthorityV1::current(
                KernelNativeSearchTierV1::T2048,
                2_026_082_601,
                valid_diagnostic_identity_v1(),
            )
            .expect("the authorized production (tier, seed) pair must construct");
            let sub_record = PopulationSearchAuthoritySlotV1::from_authority_v1(&authority);
            println!("{}", serde_json::to_string_pretty(&sub_record).unwrap());
        }

        /// Task 7 preflight, acceptance item 1: "decode-validate all 16
        /// [cycle-3] manifests through the real decoder chain-linked."
        ///
        /// Anchors at the real cycle-2 archive's own terminal link
        /// (refresh_index 18, global_generation 2,304), decoded via the
        /// evaluation-only path (this worktree does not hold tranche-1's
        /// own three genesis links, refresh_index 0-2, so a from-index-0
        /// production chain walk is not possible on this host -- disclosed
        /// in the implementation report, not silently worked around). From
        /// there, all 16 of cycle-3's OWN new manifests (refresh_index
        /// 19-34) decode through the fully unmodified, chain-checked
        /// production entry point, `decode_population_tranche_refresh_manifest_v2`,
        /// each genuinely re-validating schema, generation arithmetic,
        /// chain continuity (hash + refresh_index succession +
        /// payoff_panel_sha256 presence), frozen-slot immutability (slots
        /// 0-3, continued unchanged from the real cycle-2 terminal
        /// declaration), and slot/weight arithmetic -- this is the real
        /// gate, not a re-statement of the unit tests above.
        #[test]
        fn task7_preflight_all_16_cycle3_manifests_decode_and_chain() {
            // Coordinator-located artifacts (superseding the earlier draft
            // of this test, which bridged at refresh_index 18 via the
            // evaluation-only decoder because tranche-1's own genesis
            // links were believed missing): the real chain now walks from
            // refresh_index 0 -- tranche-1's own three genesis links
            // (D:\mtg-kernel-population-v2-tranche1\refresh\), then
            // cycle-2's real 16 (E:\c-evidence-archive-20260825\...), then
            // cycle-3's own 16 -- entirely through the fully unmodified,
            // chain-checked production decoder, never the evaluation-only
            // path.
            let dir = r"E:\mtg-kernel-population-v2-cycle3\refresh-manifests";
            let path0 = format!("{dir}\\population-v3-refresh-000.json");
            let Ok(bytes0) = std::fs::read(&path0) else {
                eprintln!("skipping: {path0} not present on this host");
                return;
            };
            let mut previous = decode_population_tranche_refresh_manifest_v2(&bytes0, None)
                .expect("refresh_index 0 (tranche-1's own genesis) must decode through the real production path");
            assert_eq!(previous.refresh_index_v2(), 0);
            assert_eq!(previous.global_generation_v2(), 0);

            // Amendment 8 (collab/CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md,
            // countersigned 96a65dce17a6546439f09e146d3dc88477d7d9a9b6adf726a2d22cf5a1d9d461):
            // idx 34's own local_gen=2048 is the schedule's terminal point,
            // consumed by no launch under the exercised consumption
            // arithmetic (local_gen(ChainThroughIndex) == ExpectedResumeLocal,
            // proven by seven real launches) -- searcher occupancy relocated
            // to idx 33 (local_gen=1920, consumed by launch 16, the cycle's
            // own final launch). This constant is itself the code-side
            // enforcement of the heavy-window schedule; it was the gate that
            // discovered A8's own resulting windows had not yet been wired
            // in here (grep-confirmed the sole site in the codebase with
            // this literal).
            const HEAVY: [u64; 4] = [20, 25, 29, 33];
            for idx in 1_u64..=34 {
                let path = format!("{dir}\\population-v3-refresh-{idx:03}.json");
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|_| panic!("{path} must be present (run Task 6's authoring script first)"));
                let decoded = decode_population_tranche_refresh_manifest_v2(&bytes, Some(&previous))
                    .unwrap_or_else(|error| {
                        panic!("refresh_index {idx} must chain-decode through the real production path: {error:?}")
                    });
                assert_eq!(decoded.refresh_index_v2(), idx);
                assert_eq!(decoded.global_generation_v2(), idx * 128);
                if idx >= 19 {
                    let slot6_is_search = decoded.slots_v2()[6].occupant_class_v2()
                        == KERNEL_NATIVE_SEARCH_AUTHORITY_KIND_V1;
                    assert_eq!(
                        slot6_is_search,
                        HEAVY.contains(&idx),
                        "refresh_index {idx}: slot 6 search-occupancy must match the heavy-window schedule exactly"
                    );
                    if slot6_is_search {
                        assert_eq!(decoded.slots_v2()[6].weight_units_v2(), 80_000);
                    }
                }
                previous = decoded;
            }
            assert_eq!(previous.refresh_index_v2(), 34);
            assert_eq!(previous.global_generation_v2(), 4_352);
            println!(
                "Task 7 preflight item 1 PASSED: the COMPLETE real chain (refresh_index 0-34, tranche-1 + \
                 cycle-2 + cycle-3's own 16) decodes and chains through the real, unmodified, chain-checked \
                 production decoder end to end; terminal manifest_sha256={}",
                lower_hex_raw32_v1(previous.manifest_sha256_v2())
            );
        }
    }
}
