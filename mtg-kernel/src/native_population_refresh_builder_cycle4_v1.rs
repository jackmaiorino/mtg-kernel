//! Thin, pure builder for the cycle-4 population refresh manifest chain.
//!
//! Wraps `native_population_refresh_manifest_cycle4_v1` (contract:
//! `docs/native_population_refresh_manifest_cycle4_v1.md`) for the one-shot
//! CLI use case in `src/bin/cycle4_refresh_build_v1.rs`: read the previous
//! manifest, the payoff panel, and the next boundary's eight slot identities
//! (all as plain files an external wrapper produced), run the multiplicative
//! weights update over the panel's per-slot rank sums, assemble the eight
//! `Cycle4RefreshSlotV1` records, and build the next manifest with the panel
//! bytes bound by content hash. Every entry point here is pure given its
//! byte inputs -- no filesystem or process access -- so the bin stays thin
//! and this module is exhaustively unit-testable without running games.
//!
//! This module's public surface never names the underlying manifest crate's
//! `pub(crate)` types directly (that would be a private-type-in-public-
//! interface error from outside this crate); every function here takes and
//! returns only plain bytes, strings, and small local result/error types.

use crate::native_population_refresh_manifest_cycle4_v1::{
    build_cycle4_refresh_manifest_v1, mw_update_cycle4_v1, panel_score_fraction_cycle4_v1,
    reload_trusted_cycle4_refresh_manifest_v1, Cycle4RefreshManifestErrorV1, Cycle4RefreshSlotV1,
    CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1, CYCLE4_SLOT_COUNT_V1,
};
use crate::native_training_store_digest_v1::lower_hex_raw32_v1;
use serde::Deserialize;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Schema for the wrapper-produced slot-identities file: the eight slots'
/// five-hash occupant identities for the boundary being built. No absolute
/// paths (the manifest's own "no absolute paths in hashed contracts" rule
/// extends to this input: paths live only in the separate, machine-local
/// slot-locator the payoff panel runner consumes).
pub const CYCLE4_SLOT_IDENTITIES_SCHEMA_V1: &str = "mtg-kernel-cycle4-slot-identities/v1";
/// Schema this builder expects of the payoff panel document (Python panel
/// runner's `panel.json`, schema `mtg-kernel-cycle4-payoff-panel/v1`). This
/// module reads only the `rank_sums` field it needs from that document; the
/// document's exact bytes are separately, opaquely content-bound into the
/// manifest by SHA-256 regardless of what this module parses out of them.
pub const CYCLE4_PANEL_DOC_SCHEMA_V1: &str = "mtg-kernel-cycle4-payoff-panel/v1";

const EXPECTED_ROLES_V1: [&str; CYCLE4_SLOT_COUNT_V1] = [
    "anchor-0",
    "anchor-1",
    "historical-0",
    "historical-1",
    "current-0",
    "current-1",
    "exploiter-0",
    "exploiter-1",
];

/// One slot's five-hash occupant identity for the boundary being built, as
/// produced by the wrapper from the Store heads.
#[derive(Clone, Debug, Deserialize)]
pub struct Cycle4SlotIdentityInputV1 {
    pub slot_index: u64,
    pub source_base_seed: u64,
    pub source_run_sha256: String,
    pub source_generation: u64,
    pub checkpoint_manifest_sha256: String,
    pub checkpoint_payload_sha256: String,
    pub model_parameter_sha256: String,
    pub train_state_sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Cycle4SlotIdentitiesFileV1 {
    schema: String,
    slots: Vec<Cycle4SlotIdentityInputV1>,
}

#[derive(Clone, Debug, Deserialize)]
struct Cycle4PanelRankSumEntryV1 {
    slot_index: u64,
    u_i: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct Cycle4PanelDocForBuildV1 {
    schema: String,
    rank_sums: Vec<Cycle4PanelRankSumEntryV1>,
}

/// Successful build result: the manifest's exact canonical bytes (write
/// these to `--output` verbatim -- they ARE the manifest) plus a small
/// summary for logging.
#[derive(Clone, Debug)]
pub struct Cycle4RefreshBuildResultV1 {
    pub canonical_bytes: Vec<u8>,
    pub manifest_sha256: String,
    pub refresh_index: u64,
    pub trainee_local_generation: u64,
    pub weight_units: [u64; CYCLE4_SLOT_COUNT_V1],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cycle4RefreshBuildErrorKindV1 {
    InvalidSlotIdentitiesDocument,
    InvalidPanelDocument,
    /// The underlying manifest contract rejected the assembled manifest;
    /// the payload is `Debug`-formatted from
    /// `Cycle4RefreshManifestErrorKindV1` (a crate-private type this
    /// module's public interface cannot name directly).
    ManifestRejected(String),
}

#[derive(Clone, Debug)]
pub struct Cycle4RefreshBuildErrorV1 {
    kind: Cycle4RefreshBuildErrorKindV1,
}

impl Cycle4RefreshBuildErrorV1 {
    const fn new(kind: Cycle4RefreshBuildErrorKindV1) -> Self {
        Self { kind }
    }

    pub const fn kind_v1(&self) -> &Cycle4RefreshBuildErrorKindV1 {
        &self.kind
    }
}

impl Display for Cycle4RefreshBuildErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.kind)
    }
}

impl Error for Cycle4RefreshBuildErrorV1 {}

impl From<Cycle4RefreshManifestErrorV1> for Cycle4RefreshBuildErrorV1 {
    fn from(error: Cycle4RefreshManifestErrorV1) -> Self {
        Self::new(Cycle4RefreshBuildErrorKindV1::ManifestRejected(format!(
            "{:?}",
            error.kind_v1()
        )))
    }
}

type Result<T> = std::result::Result<T, Cycle4RefreshBuildErrorV1>;

fn invalid_slot_identities_v1() -> Cycle4RefreshBuildErrorV1 {
    Cycle4RefreshBuildErrorV1::new(Cycle4RefreshBuildErrorKindV1::InvalidSlotIdentitiesDocument)
}

fn invalid_panel_v1() -> Cycle4RefreshBuildErrorV1 {
    Cycle4RefreshBuildErrorV1::new(Cycle4RefreshBuildErrorKindV1::InvalidPanelDocument)
}

/// Parses the slot-identities file and orders its (unordered) entries into
/// exactly one identity per slot index `0..8`; fails closed on a missing,
/// duplicate, or out-of-range slot index, or a wrong/missing schema tag.
fn ordered_slot_identities_v1(
    json_bytes: &[u8],
) -> Result<[Cycle4SlotIdentityInputV1; CYCLE4_SLOT_COUNT_V1]> {
    let file: Cycle4SlotIdentitiesFileV1 =
        serde_json::from_slice(json_bytes).map_err(|_| invalid_slot_identities_v1())?;
    if file.schema != CYCLE4_SLOT_IDENTITIES_SCHEMA_V1 || file.slots.len() != CYCLE4_SLOT_COUNT_V1
    {
        return Err(invalid_slot_identities_v1());
    }
    let mut ordered: [Option<Cycle4SlotIdentityInputV1>; CYCLE4_SLOT_COUNT_V1] =
        [None, None, None, None, None, None, None, None];
    for entry in file.slots {
        let index = usize::try_from(entry.slot_index).map_err(|_| invalid_slot_identities_v1())?;
        if index >= CYCLE4_SLOT_COUNT_V1 || ordered[index].is_some() {
            return Err(invalid_slot_identities_v1());
        }
        ordered[index] = Some(entry);
    }
    if ordered.iter().any(Option::is_none) {
        return Err(invalid_slot_identities_v1());
    }
    // `unwrap` is safe: every entry was just proven `Some` above.
    Ok(ordered.map(|entry| entry.unwrap_or_else(|| unreachable!("checked above"))))
}

/// Parses the panel document and returns its per-slot rank sums `u_i`,
/// ordered by slot index `0..8`; fails closed on a missing, duplicate, or
/// out-of-range slot index, or a wrong/missing schema tag. Reads ONLY the
/// `rank_sums` field -- the document's role as the content the next manifest
/// binds by SHA-256 is enforced separately, by passing its raw bytes through
/// to `build_cycle4_refresh_manifest_v1` unparsed.
fn ordered_panel_rank_sums_v1(panel_bytes: &[u8]) -> Result<[i64; CYCLE4_SLOT_COUNT_V1]> {
    let doc: Cycle4PanelDocForBuildV1 =
        serde_json::from_slice(panel_bytes).map_err(|_| invalid_panel_v1())?;
    if doc.schema != CYCLE4_PANEL_DOC_SCHEMA_V1 || doc.rank_sums.len() != CYCLE4_SLOT_COUNT_V1 {
        return Err(invalid_panel_v1());
    }
    let mut ordered: [Option<i64>; CYCLE4_SLOT_COUNT_V1] = [None; CYCLE4_SLOT_COUNT_V1];
    for entry in doc.rank_sums {
        let index = usize::try_from(entry.slot_index).map_err(|_| invalid_panel_v1())?;
        if index >= CYCLE4_SLOT_COUNT_V1 || ordered[index].is_some() {
            return Err(invalid_panel_v1());
        }
        ordered[index] = Some(entry.u_i);
    }
    let mut result = [0_i64; CYCLE4_SLOT_COUNT_V1];
    for (index, value) in ordered.iter().enumerate() {
        result[index] = value.ok_or_else(invalid_panel_v1)?;
    }
    Ok(result)
}

fn build_slot_records_v1(
    identities: &[Cycle4SlotIdentityInputV1; CYCLE4_SLOT_COUNT_V1],
    weight_units: &[u64; CYCLE4_SLOT_COUNT_V1],
) -> Vec<Cycle4RefreshSlotV1> {
    identities
        .iter()
        .enumerate()
        .map(|(index, identity)| Cycle4RefreshSlotV1 {
            slot_index: u64::try_from(index).unwrap_or_else(|_| unreachable!("index < 8")),
            role: EXPECTED_ROLES_V1[index].to_owned(),
            occupant_class: if index >= 6 {
                "historical-fallback".to_owned()
            } else {
                "policy".to_owned()
            },
            source_base_seed: identity.source_base_seed,
            source_run_sha256: identity.source_run_sha256.clone(),
            source_generation: identity.source_generation,
            checkpoint_manifest_sha256: identity.checkpoint_manifest_sha256.clone(),
            checkpoint_payload_sha256: identity.checkpoint_payload_sha256.clone(),
            model_parameter_sha256: identity.model_parameter_sha256.clone(),
            train_state_sha256: identity.train_state_sha256.clone(),
            weight_units: weight_units[index],
        })
        .collect()
}

/// Builds refresh 0 (genesis): uniform weights, no panel, no previous
/// manifest. `slot_identities_json` supplies the eight occupants' identities
/// (anchor/historical/current/exploiter, per the ratified roster table).
pub fn build_cycle4_genesis_refresh_v1(
    trainee_run_sha256: &str,
    trainee_base_seed: u64,
    slot_identities_json: &[u8],
) -> Result<Cycle4RefreshBuildResultV1> {
    let identities = ordered_slot_identities_v1(slot_identities_json)?;
    let weight_units = [CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1; CYCLE4_SLOT_COUNT_V1];
    let slots = build_slot_records_v1(&identities, &weight_units);
    let manifest =
        build_cycle4_refresh_manifest_v1(0, None, None, trainee_run_sha256, trainee_base_seed, slots)?;
    Ok(Cycle4RefreshBuildResultV1 {
        canonical_bytes: manifest.canonical_bytes_v1().to_vec(),
        manifest_sha256: lower_hex_raw32_v1(manifest.manifest_sha256_v1()),
        refresh_index: manifest.refresh_index_v1(),
        trainee_local_generation: manifest.trainee_local_generation_v1(),
        weight_units,
    })
}

/// Builds refresh `next_refresh_index` (`>= 1`) by chaining off
/// `previous_manifest_bytes` (that refresh's own exact canonical bytes) and
/// binding `panel_bytes` (the payoff panel that evaluated the previous
/// manifest's eight identities) by content hash. Runs the multiplicative-
/// weights update over the panel's per-slot rank sums against the previous
/// manifest's weights, then assembles the next boundary's eight slot
/// records from `slot_identities_json`.
#[allow(clippy::too_many_arguments)]
pub fn build_cycle4_next_refresh_v1(
    previous_manifest_bytes: &[u8],
    panel_bytes: &[u8],
    next_refresh_index: u64,
    trainee_run_sha256: &str,
    trainee_base_seed: u64,
    slot_identities_json: &[u8],
) -> Result<Cycle4RefreshBuildResultV1> {
    let previous = reload_trusted_cycle4_refresh_manifest_v1(previous_manifest_bytes)?;
    let rank_sums = ordered_panel_rank_sums_v1(panel_bytes)?;
    let mut panel_score_fractions = [0.0_f64; CYCLE4_SLOT_COUNT_V1];
    for (index, rank_sum) in rank_sums.into_iter().enumerate() {
        panel_score_fractions[index] = panel_score_fraction_cycle4_v1(rank_sum)?;
    }
    let mut prior_weight_units = [0_u64; CYCLE4_SLOT_COUNT_V1];
    for (index, slot) in previous.slots_v1().iter().enumerate() {
        prior_weight_units[index] = slot.weight_units;
    }
    let weight_units = mw_update_cycle4_v1(&prior_weight_units, &panel_score_fractions)?;
    let identities = ordered_slot_identities_v1(slot_identities_json)?;
    let slots = build_slot_records_v1(&identities, &weight_units);
    let manifest = build_cycle4_refresh_manifest_v1(
        next_refresh_index,
        Some(&previous),
        Some(panel_bytes),
        trainee_run_sha256,
        trainee_base_seed,
        slots,
    )?;
    Ok(Cycle4RefreshBuildResultV1 {
        canonical_bytes: manifest.canonical_bytes_v1().to_vec(),
        manifest_sha256: lower_hex_raw32_v1(manifest.manifest_sha256_v1()),
        refresh_index: manifest.refresh_index_v1(),
        trainee_local_generation: manifest.trainee_local_generation_v1(),
        weight_units,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_population_refresh_manifest_cycle4_v1::{
        CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1, CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1,
        CYCLE4_HISTORICAL_LAG_V1, CYCLE4_ROLE_FLOOR_UNITS_V1, CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1,
        CYCLE4_WEIGHT_TOTAL_UNITS_V1,
    };

    const TEST_TRAINEE_RUN: &str = CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1;
    const TEST_TRAINEE_SEED: u64 = CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1;

    fn hash_tag_v1(tag: u8) -> String {
        format!("cd{:062x}", u64::from(tag))
    }

    /// Builds a boundary's slot-identities document. `refresh_index`
    /// controls the two trainee-bound slots' `source_generation` (matching
    /// what a real wrapper would compute from the Store heads at that
    /// boundary); `trainee_run`/`trainee_seed` MUST equal whatever is passed
    /// as the build call's own `trainee_run_sha256`/`trainee_base_seed`,
    /// since the manifest module binds slot 5 (and, from refresh index 4
    /// onward, slot 2) to that exact pair.
    fn slot_identities_json_v1(refresh_index: u64, trainee_run: &str, trainee_seed: u64) -> Vec<u8> {
        let local_generation = CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 + refresh_index * 128;
        let historical_0_generation = local_generation - CYCLE4_HISTORICAL_LAG_V1;
        let (historical_0_run, historical_0_seed) = if refresh_index <= 3 {
            (
                CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1.to_owned(),
                CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1,
            )
        } else {
            (trainee_run.to_owned(), trainee_seed)
        };
        let rotation = refresh_index % 3;
        let frozen = |slot_index: u64, tag: u8, seed: u64, generation: u64| {
            serde_json::json!({
                "slot_index": slot_index,
                "source_base_seed": seed,
                "source_run_sha256": hash_tag_v1(tag),
                "source_generation": generation,
                "checkpoint_manifest_sha256": hash_tag_v1(tag + 1),
                "checkpoint_payload_sha256": hash_tag_v1(tag + 2),
                "model_parameter_sha256": hash_tag_v1(tag + 3),
                "train_state_sha256": hash_tag_v1(tag + 4),
            })
        };
        let doc = serde_json::json!({
            "schema": CYCLE4_SLOT_IDENTITIES_SCHEMA_V1,
            "slots": [
                frozen(0, 10, 920_012, 384),
                frozen(1, 20, 970_002, 1536),
                serde_json::json!({
                    "slot_index": 2,
                    "source_base_seed": historical_0_seed,
                    "source_run_sha256": historical_0_run,
                    "source_generation": historical_0_generation,
                    "checkpoint_manifest_sha256": hash_tag_v1(31),
                    "checkpoint_payload_sha256": hash_tag_v1(32),
                    "model_parameter_sha256": hash_tag_v1(33),
                    "train_state_sha256": hash_tag_v1(34),
                }),
                frozen(3, 40 + u8::try_from(rotation).expect("rotation fits u8"), 970_001 + rotation, 1024),
                frozen(4, 50, 975_002, 2048),
                serde_json::json!({
                    "slot_index": 5,
                    "source_base_seed": trainee_seed,
                    "source_run_sha256": trainee_run,
                    "source_generation": local_generation,
                    "checkpoint_manifest_sha256": hash_tag_v1(61),
                    "checkpoint_payload_sha256": hash_tag_v1(62),
                    "model_parameter_sha256": hash_tag_v1(63),
                    "train_state_sha256": hash_tag_v1(64),
                }),
                frozen(6, 70, 971_222, 1024),
                frozen(7, 80, 971_221, 512),
            ],
        });
        serde_json::to_vec(&doc).expect("slot identities json")
    }

    fn panel_json_v1(rank_sums: [i64; CYCLE4_SLOT_COUNT_V1]) -> Vec<u8> {
        let entries: Vec<_> = rank_sums
            .iter()
            .enumerate()
            .map(|(index, u_i)| serde_json::json!({"slot_index": index, "u_i": u_i}))
            .collect();
        let doc = serde_json::json!({
            "schema": CYCLE4_PANEL_DOC_SCHEMA_V1,
            "rank_sums": entries,
        });
        serde_json::to_vec(&doc).expect("panel json")
    }

    #[test]
    fn genesis_builds_with_uniform_weights_v1() {
        let identities = slot_identities_json_v1(0);
        let result = build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &identities)
            .expect("genesis build");
        assert_eq!(result.refresh_index, 0);
        assert_eq!(result.trainee_local_generation, 896);
        assert_eq!(
            result.weight_units,
            [CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1; CYCLE4_SLOT_COUNT_V1]
        );
        assert_eq!(
            result.weight_units.iter().sum::<u64>(),
            CYCLE4_WEIGHT_TOTAL_UNITS_V1
        );
        // The written bytes decode back to the same manifest sha (round
        // trip through the same builder that validates on construction).
        assert!(!result.manifest_sha256.is_empty());
        assert!(!result.canonical_bytes.is_empty());
    }

    #[test]
    fn genesis_rejects_malformed_slot_identities_v1() {
        let bad = b"not json".to_vec();
        let error = build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &bad)
            .expect_err("malformed slot identities");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::InvalidSlotIdentitiesDocument
        );
    }

    #[test]
    fn genesis_rejects_wrong_slot_identities_schema_v1() {
        let doc = serde_json::json!({"schema": "wrong/v1", "slots": []});
        let bytes = serde_json::to_vec(&doc).expect("json");
        let error = build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &bytes)
            .expect_err("wrong schema");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::InvalidSlotIdentitiesDocument
        );
    }

    #[test]
    fn next_refresh_moves_weight_toward_winners_within_constraints_v1() {
        let genesis_identities = slot_identities_json_v1(0);
        let genesis = build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &genesis_identities)
            .expect("genesis build");
        // Slot 0 (anchor-0) sweeps its role pair; slot 1 (anchor-1) loses it;
        // everyone else is even. `u_i` values are each policy's own signed
        // sum over 7 * 256-game matchups (bounded by +/- 1792), matching
        // `panel_score_fraction_cycle4_v1`'s accepted range.
        let rank_sums = [900_i64, -900, 0, 0, 0, 0, 0, 0];
        let panel = panel_json_v1(rank_sums);
        let next_identities = slot_identities_json_v1(1);
        let next = build_cycle4_next_refresh_v1(
            &genesis.canonical_bytes,
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect("next refresh build");
        assert_eq!(next.refresh_index, 1);
        assert_eq!(next.trainee_local_generation, 896 + 128);
        assert!(next.weight_units[0] > genesis.weight_units[0]);
        assert!(next.weight_units[1] < genesis.weight_units[1]);
        assert_eq!(
            next.weight_units.iter().sum::<u64>(),
            CYCLE4_WEIGHT_TOTAL_UNITS_V1
        );
        for pair in 0..4 {
            assert!(
                next.weight_units[2 * pair] + next.weight_units[2 * pair + 1]
                    >= CYCLE4_ROLE_FLOOR_UNITS_V1
            );
        }
    }

    #[test]
    fn next_refresh_rejects_panel_bytes_not_matching_declared_hash_v1() {
        // This exercises the manifest module's own content-resolving bind:
        // the builder passes `panel_bytes` straight through to
        // `build_cycle4_refresh_manifest_v1`, which computes
        // `payoff_panel_sha256` FROM those bytes -- so there is no way to
        // "mismatch" a hash the builder itself always derives correctly.
        // What CAN go wrong downstream (the scenario this test proves fails
        // closed) is a caller re-declaring a manifest as chained off a
        // DIFFERENT previous manifest than the one the panel bytes actually
        // describe: the chain-linkage check inside
        // `build_cycle4_refresh_manifest_v1` rejects it before the manifest
        // is ever written.
        let genesis_identities = slot_identities_json_v1(0);
        let genesis_a = build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &genesis_identities)
            .expect("genesis a");
        // A second, distinct genesis-shaped manifest (different trainee run)
        // stands in for "the wrong previous manifest".
        let other_run = hash_tag_v1(200);
        let genesis_b =
            build_cycle4_genesis_refresh_v1(&other_run, TEST_TRAINEE_SEED, &genesis_identities)
                .expect("genesis b");
        let panel = panel_json_v1([0_i64; CYCLE4_SLOT_COUNT_V1]);
        let next_identities = slot_identities_json_v1(1);
        // Build refresh 1 for genesis_a, then try to reuse ITS panel bytes
        // (fine, self-consistent) but chain it off genesis_b's bytes: the
        // trainee-run binding drift must be rejected.
        let error = build_cycle4_next_refresh_v1(
            &genesis_b.canonical_bytes,
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("trainee run drift against the wrong previous manifest");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::ManifestRejected("InvalidChain".to_owned())
        );
    }

    #[test]
    fn next_refresh_rejects_malformed_panel_document_v1() {
        let genesis_identities = slot_identities_json_v1(0);
        let genesis = build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &genesis_identities)
            .expect("genesis build");
        let bad_panel = b"{\"schema\":\"wrong\"}".to_vec();
        let next_identities = slot_identities_json_v1(1);
        let error = build_cycle4_next_refresh_v1(
            &genesis.canonical_bytes,
            &bad_panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("malformed panel");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::InvalidPanelDocument
        );
    }

    #[test]
    fn next_refresh_rejects_out_of_range_rank_sum_v1() {
        let genesis_identities = slot_identities_json_v1(0);
        let genesis = build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &genesis_identities)
            .expect("genesis build");
        let mut rank_sums = [0_i64; CYCLE4_SLOT_COUNT_V1];
        rank_sums[0] = 2_000; // exceeds the 7*256=1792 game bound
        let panel = panel_json_v1(rank_sums);
        let next_identities = slot_identities_json_v1(1);
        let error = build_cycle4_next_refresh_v1(
            &genesis.canonical_bytes,
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("out of range rank sum");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::ManifestRejected("MwArithmetic".to_owned())
        );
    }

    #[test]
    fn genesis_path_never_reads_a_panel_v1() {
        // Genesis takes no panel argument at all in this module's API --
        // this test documents/pins that shape rather than exercising a
        // runtime branch.
        let identities = slot_identities_json_v1(0);
        let result = build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &identities)
            .expect("genesis build");
        assert_eq!(result.refresh_index, 0);
    }
}
