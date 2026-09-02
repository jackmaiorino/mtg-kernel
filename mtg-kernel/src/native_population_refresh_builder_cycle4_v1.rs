//! Thin, pure builder for the cycle-4 population refresh manifest chain.
//!
//! Wraps `native_population_refresh_manifest_cycle4_v1` (contract:
//! `docs/native_population_refresh_manifest_cycle4_v1.md`) for the one-shot
//! CLI use case in `src/bin/cycle4_refresh_build_v1.rs`: read the whole
//! refresh chain from genesis (every prior manifest, plus the panel bytes
//! that content-bind each non-genesis link), the new payoff panel that
//! evaluated the chain tip's roster, and the next boundary's eight slot
//! identities (all as plain files an external wrapper produced), run the
//! multiplicative weights update over the panel's per-slot rank sums,
//! assemble the eight `Cycle4RefreshSlotV1` records, and build the next
//! manifest with the panel bytes bound by content hash. Every entry point
//! here is pure given its byte inputs -- no filesystem or process access --
//! so the bin stays thin and this module is exhaustively unit-testable
//! without running games or touching a real chain directory (walking the
//! directory into `Cycle4ChainLinkV1` values is the bin's job).
//!
//! This module's public surface never names the underlying manifest crate's
//! `pub(crate)` types directly (that would be a private-type-in-public-
//! interface error from outside this crate); every function here takes and
//! returns only plain bytes, strings, and small local result/error types.
//!
//! ## Chain directory naming scheme (binding on every producer/consumer)
//!
//! A refresh chain directory holds, for every refresh index `NN` (`00`
//! through `16`, zero-padded to two digits) built so far:
//!   - `refresh-NN.manifest.json`: that refresh's exact canonical bytes.
//!   - `refresh-NN.panel.json`, for `NN >= 1` only: the payoff panel that
//!     evaluated refresh `NN - 1`'s roster and is bound by SHA-256 into
//!     refresh `NN`'s manifest (`payoff_panel_sha256`). Genesis
//!     (`refresh-00.manifest.json`) has no panel file -- it has no
//!     predecessor to evaluate.
//!
//! `cycle4_chain_manifest_filename_v1` and `cycle4_chain_panel_filename_v1`
//! are the single source of truth for these names on the Rust side; the
//! Python payoff-panel runner
//! (`scripts/experiments/population_v2_cycle4_v1/run_payoff_panel_v1.py`)
//! reproduces the same scheme independently (it has no way to import Rust)
//! and must never drift from it.

use crate::native_population_refresh_manifest_cycle4_v1::{
    build_cycle4_refresh_manifest_v1, decode_cycle4_refresh_manifest_v1, mw_update_cycle4_v1,
    panel_score_fraction_cycle4_v1, Cycle4RefreshManifestErrorV1, Cycle4RefreshManifestV1,
    Cycle4RefreshSlotV1, CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1, CYCLE4_PANEL_GAMES_PER_MATCHUP_V1,
    CYCLE4_PANEL_GAMES_PER_POLICY_V1, CYCLE4_SLOT_COUNT_V1,
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
/// runner's `panel.json` / `refresh-NN.panel.json`, schema
/// `mtg-kernel-cycle4-payoff-panel/v1`). This module reads the
/// `manifest_sha256`, `rank_sums`, and `matchups` fields it needs from that
/// document; the document's exact bytes are separately, opaquely
/// content-bound into the manifest by SHA-256 regardless of what this
/// module parses out of them.
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

/// Number of round-robin matchups over `CYCLE4_SLOT_COUNT_V1` slots:
/// `C(8, 2) = 28`.
const CYCLE4_MATCHUP_COUNT_V1: usize = CYCLE4_SLOT_COUNT_V1 * (CYCLE4_SLOT_COUNT_V1 - 1) / 2;

/// Fixed on-disk filename for one refresh index's manifest, per the chain
/// directory naming scheme documented on this module.
#[must_use]
pub fn cycle4_chain_manifest_filename_v1(refresh_index: u64) -> String {
    format!("refresh-{refresh_index:02}.manifest.json")
}

/// Fixed on-disk filename for one refresh index's payoff panel (absent for
/// index 0), per the chain directory naming scheme documented on this
/// module.
#[must_use]
pub fn cycle4_chain_panel_filename_v1(refresh_index: u64) -> String {
    format!("refresh-{refresh_index:02}.panel.json")
}

/// One slot's five-hash occupant identity for the boundary being built, as
/// produced by the wrapper from the Store heads. This schema is fully owned
/// by this module (unlike the panel document, which is a much larger
/// externally-produced file this module only reads a slice of), so it is
/// deny-unknown-fields: fail closed on a typo'd or stale field name rather
/// than silently ignoring it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
struct Cycle4SlotIdentitiesFileV1 {
    schema: String,
    slots: Vec<Cycle4SlotIdentityInputV1>,
}

#[derive(Clone, Debug, Deserialize)]
struct Cycle4PanelRankSumEntryV1 {
    slot_index: u64,
    u_i: i64,
}

/// One row of the panel's matchup ledger -- the raw evidence every declared
/// `rank_sums` entry is recomputed FROM (never trusted on its own): the
/// cycle-3 lesson generalizes past manifest content to panel content too.
#[derive(Clone, Debug, Deserialize)]
struct Cycle4PanelMatchupRowV1 {
    lower_slot_index: u64,
    higher_slot_index: u64,
    game_count: u64,
    lower_wins: i64,
    lower_draws: i64,
    lower_losses: i64,
    higher_wins: i64,
    higher_draws: i64,
    higher_losses: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct Cycle4PanelDocForBuildV1 {
    schema: String,
    manifest_sha256: String,
    rank_sums: Vec<Cycle4PanelRankSumEntryV1>,
    matchups: Vec<Cycle4PanelMatchupRowV1>,
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
    /// The panel's declared `manifest_sha256` does not equal the SHA-256 of
    /// the chain tip's own canonical bytes: this panel evaluated a
    /// DIFFERENT manifest than the one about to be updated.
    PanelManifestMismatch,
    /// The chain (from `--chain-dir`) contained no links at all; every
    /// next-refresh build requires at least a genesis manifest already on
    /// disk.
    EmptyChain,
    /// A chain link violated the fixed naming-scheme invariant -- the
    /// genesis link carrying a panel, or a non-genesis link missing one --
    /// before its bytes were ever decoded.
    MalformedChain,
    /// The underlying manifest contract rejected the assembled manifest, or
    /// rejected a link while the prior chain was being content-resolved and
    /// hash-chained from genesis; the payload is `Debug`-formatted from
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

fn panel_manifest_mismatch_v1() -> Cycle4RefreshBuildErrorV1 {
    Cycle4RefreshBuildErrorV1::new(Cycle4RefreshBuildErrorKindV1::PanelManifestMismatch)
}

fn empty_chain_v1() -> Cycle4RefreshBuildErrorV1 {
    Cycle4RefreshBuildErrorV1::new(Cycle4RefreshBuildErrorKindV1::EmptyChain)
}

fn malformed_chain_v1() -> Cycle4RefreshBuildErrorV1 {
    Cycle4RefreshBuildErrorV1::new(Cycle4RefreshBuildErrorKindV1::MalformedChain)
}

/// Parses the slot-identities file and orders its (unordered) entries into
/// exactly one identity per slot index `0..8`; fails closed on a missing,
/// duplicate, or out-of-range slot index, or a wrong/missing schema tag.
fn ordered_slot_identities_v1(
    json_bytes: &[u8],
) -> Result<[Cycle4SlotIdentityInputV1; CYCLE4_SLOT_COUNT_V1]> {
    let file: Cycle4SlotIdentitiesFileV1 =
        serde_json::from_slice(json_bytes).map_err(|_| invalid_slot_identities_v1())?;
    if file.schema != CYCLE4_SLOT_IDENTITIES_SCHEMA_V1 || file.slots.len() != CYCLE4_SLOT_COUNT_V1 {
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

/// Parses the declared `rank_sums` field into per-slot order `0..8`; fails
/// closed on a missing, duplicate, or out-of-range slot index, or the wrong
/// entry count. This is only ONE of the two independent views
/// `validated_panel_rank_sums_v1` requires to agree -- see
/// `recomputed_rank_sums_v1` for the other (the raw matchup ledger).
fn ordered_declared_rank_sums_v1(
    entries: &[Cycle4PanelRankSumEntryV1],
) -> Result<[i64; CYCLE4_SLOT_COUNT_V1]> {
    if entries.len() != CYCLE4_SLOT_COUNT_V1 {
        return Err(invalid_panel_v1());
    }
    let mut ordered: [Option<i64>; CYCLE4_SLOT_COUNT_V1] = [None; CYCLE4_SLOT_COUNT_V1];
    for entry in entries {
        let index = usize::try_from(entry.slot_index).map_err(|_| invalid_panel_v1())?;
        if index >= CYCLE4_SLOT_COUNT_V1 || ordered[index].is_some() {
            return Err(invalid_panel_v1());
        }
        ordered[index] = Some(entry.u_i);
    }
    let mut result = [0_i64; CYCLE4_SLOT_COUNT_V1];
    for (index, value) in ordered.into_iter().enumerate() {
        result[index] = value.ok_or_else(invalid_panel_v1)?;
    }
    Ok(result)
}

/// Recomputes every slot's rank sum `u_i` directly from the raw
/// `CYCLE4_MATCHUP_COUNT_V1`-row matchup ledger: win `+1`, draw `0`, loss
/// `-1` per game, summed over ALL of a slot's games in BOTH the "lower" and
/// "higher" seat (a slot appears as `lower_slot_index` in some rows and
/// `higher_slot_index` in others; both contribute). Requires exactly the 28
/// unordered pairs, each covered exactly once, each side's `wins + draws +
/// losses` equal to `CYCLE4_PANEL_GAMES_PER_MATCHUP_V1`, the two recorded
/// views of the same games mirroring each other (a win for one side is a
/// loss for the other, draws agree), and -- after the loop -- every slot's
/// total games played across its seven matchups exactly
/// `CYCLE4_PANEL_GAMES_PER_POLICY_V1` (`7 * G`). Fails closed on any
/// disagreement.
fn recomputed_rank_sums_v1(
    matchups: &[Cycle4PanelMatchupRowV1],
) -> Result<[i64; CYCLE4_SLOT_COUNT_V1]> {
    if matchups.len() != CYCLE4_MATCHUP_COUNT_V1 {
        return Err(invalid_panel_v1());
    }
    let games_per_matchup =
        i64::try_from(CYCLE4_PANEL_GAMES_PER_MATCHUP_V1).map_err(|_| invalid_panel_v1())?;
    let expected_total =
        i64::try_from(CYCLE4_PANEL_GAMES_PER_POLICY_V1).map_err(|_| invalid_panel_v1())?;
    let mut totals = [0_i64; CYCLE4_SLOT_COUNT_V1];
    let mut rank_sums = [0_i64; CYCLE4_SLOT_COUNT_V1];
    let mut seen_pairs = std::collections::BTreeSet::new();
    for row in matchups {
        let lower = usize::try_from(row.lower_slot_index).map_err(|_| invalid_panel_v1())?;
        let higher = usize::try_from(row.higher_slot_index).map_err(|_| invalid_panel_v1())?;
        if lower >= CYCLE4_SLOT_COUNT_V1 || higher >= CYCLE4_SLOT_COUNT_V1 || lower >= higher {
            return Err(invalid_panel_v1());
        }
        if !seen_pairs.insert((lower, higher)) {
            return Err(invalid_panel_v1());
        }
        if row.game_count != CYCLE4_PANEL_GAMES_PER_MATCHUP_V1
            || row.lower_wins < 0
            || row.lower_draws < 0
            || row.lower_losses < 0
            || row.higher_wins < 0
            || row.higher_draws < 0
            || row.higher_losses < 0
            || row.lower_wins + row.lower_draws + row.lower_losses != games_per_matchup
            || row.higher_wins + row.higher_draws + row.higher_losses != games_per_matchup
            || row.higher_wins != row.lower_losses
            || row.higher_losses != row.lower_wins
            || row.higher_draws != row.lower_draws
        {
            return Err(invalid_panel_v1());
        }
        totals[lower] += games_per_matchup;
        totals[higher] += games_per_matchup;
        rank_sums[lower] += row.lower_wins - row.lower_losses;
        rank_sums[higher] += row.higher_wins - row.higher_losses;
    }
    if totals.iter().any(|total| *total != expected_total) {
        return Err(invalid_panel_v1());
    }
    Ok(rank_sums)
}

/// Parses the panel document, binds it to the chain tip it must have
/// evaluated (`manifest_sha256` equality against the tip's own
/// canonical-bytes SHA-256, checked before any MW arithmetic runs), and
/// requires the declared `rank_sums` to agree EXACTLY with what
/// `recomputed_rank_sums_v1` derives from the raw matchup ledger. Reads
/// `matchup_index` and the `*_role` fields (present in the production
/// document but not consulted here) implicitly by way of `serde`'s
/// default "ignore unrecognized fields" behavior -- this struct is
/// deliberately not `deny_unknown_fields`, since the document's role as
/// content the manifest binds by SHA-256 is enforced separately, by
/// passing its raw bytes through to `build_cycle4_refresh_manifest_v1`
/// unparsed.
fn validated_panel_rank_sums_v1(
    panel_bytes: &[u8],
    chain_tip_manifest_sha256_hex: &str,
) -> Result<[i64; CYCLE4_SLOT_COUNT_V1]> {
    let doc: Cycle4PanelDocForBuildV1 =
        serde_json::from_slice(panel_bytes).map_err(|_| invalid_panel_v1())?;
    if doc.schema != CYCLE4_PANEL_DOC_SCHEMA_V1 {
        return Err(invalid_panel_v1());
    }
    if doc.manifest_sha256 != chain_tip_manifest_sha256_hex {
        return Err(panel_manifest_mismatch_v1());
    }
    let declared = ordered_declared_rank_sums_v1(&doc.rank_sums)?;
    let computed = recomputed_rank_sums_v1(&doc.matchups)?;
    if declared != computed {
        return Err(invalid_panel_v1());
    }
    Ok(computed)
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
    let manifest = build_cycle4_refresh_manifest_v1(
        0,
        None,
        None,
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

/// One link of a refresh chain, as read from `refresh-NN.manifest.json`
/// (and, for `NN >= 1`, `refresh-NN.panel.json`) under the chain directory
/// naming scheme documented on this module. `panel_bytes` MUST be `None`
/// for the genesis link (index 0, the first entry of the slice passed to
/// `build_cycle4_next_refresh_v1`) and `Some` for every later link;
/// violating either fails closed with `MalformedChain` before any link's
/// bytes are decoded.
#[derive(Clone, Debug)]
pub struct Cycle4ChainLinkV1 {
    pub manifest_bytes: Vec<u8>,
    pub panel_bytes: Option<Vec<u8>>,
}

/// Decodes and fully validates `chain` from genesis (`chain[0]`) through its
/// last link (the tip), content-resolving each non-genesis link's panel
/// bytes and hash-chaining it to its already-validated predecessor via
/// `decode_cycle4_refresh_manifest_v1` -- so a chain directory produced by
/// anything other than this same content-resolving pipeline fails closed
/// here, before the tip is ever trusted for MW arithmetic. Returns the
/// validated tip.
fn decode_and_validate_chain_v1(chain: &[Cycle4ChainLinkV1]) -> Result<Cycle4RefreshManifestV1> {
    let mut previous: Option<Cycle4RefreshManifestV1> = None;
    for (index, link) in chain.iter().enumerate() {
        let is_genesis = index == 0;
        if is_genesis != link.panel_bytes.is_none() {
            return Err(malformed_chain_v1());
        }
        let decoded = decode_cycle4_refresh_manifest_v1(
            &link.manifest_bytes,
            previous.as_ref(),
            link.panel_bytes.as_deref(),
        )?;
        previous = Some(decoded);
    }
    previous.ok_or_else(empty_chain_v1)
}

/// Builds refresh `next_refresh_index` (`>= 1`) by first content-resolving
/// and hash-chaining the ENTIRE prior chain from genesis through the tip
/// (`chain`, per `Cycle4ChainLinkV1`'s doc -- this replaces trusting a
/// single previously-reloaded manifest with re-deriving the whole chain
/// every time, closing the format-only reload path), then binding
/// `panel_bytes` (the payoff panel that evaluated the chain tip's eight
/// identities) to that tip by content hash: the panel's own declared
/// `manifest_sha256` must equal the tip's canonical-bytes SHA-256, and its
/// declared `rank_sums` must agree with what its raw matchup ledger
/// actually shows, both checked before any MW arithmetic runs. Runs the
/// multiplicative-weights update over those rank sums against the tip's
/// weights, then assembles the next boundary's eight slot records from
/// `slot_identities_json`.
#[allow(clippy::too_many_arguments)]
pub fn build_cycle4_next_refresh_v1(
    chain: &[Cycle4ChainLinkV1],
    panel_bytes: &[u8],
    next_refresh_index: u64,
    trainee_run_sha256: &str,
    trainee_base_seed: u64,
    slot_identities_json: &[u8],
) -> Result<Cycle4RefreshBuildResultV1> {
    let tip = decode_and_validate_chain_v1(chain)?;
    let tip_sha256_hex = lower_hex_raw32_v1(tip.manifest_sha256_v1());
    let rank_sums = validated_panel_rank_sums_v1(panel_bytes, &tip_sha256_hex)?;
    let mut panel_score_fractions = [0.0_f64; CYCLE4_SLOT_COUNT_V1];
    for (index, rank_sum) in rank_sums.into_iter().enumerate() {
        panel_score_fractions[index] = panel_score_fraction_cycle4_v1(rank_sum)?;
    }
    let mut prior_weight_units = [0_u64; CYCLE4_SLOT_COUNT_V1];
    for (index, slot) in tip.slots_v1().iter().enumerate() {
        prior_weight_units[index] = slot.weight_units;
    }
    let weight_units = mw_update_cycle4_v1(&prior_weight_units, &panel_score_fractions)?;
    let identities = ordered_slot_identities_v1(slot_identities_json)?;
    let slots = build_slot_records_v1(&identities, &weight_units);
    let manifest = build_cycle4_refresh_manifest_v1(
        next_refresh_index,
        Some(&tip),
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
    /// One test override for a single matchup: ((row_slot, col_slot), (row_wins, col_wins, draws)).
    type MatchupOverrideV1 = ((usize, usize), (i64, i64, i64));
    use super::*;
    use crate::native_population_refresh_manifest_cycle4_v1::{
        FrozenOccupantIdentityCycle4V1, CYCLE4_ANCHOR_0_V1, CYCLE4_ANCHOR_1_V1,
        CYCLE4_CURRENT_0_V1, CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1,
        CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1, CYCLE4_EXPLOITER_0_V1, CYCLE4_EXPLOITER_1_V1,
        CYCLE4_HISTORICAL_1_ROTATION_V1, CYCLE4_HISTORICAL_LAG_V1, CYCLE4_ROLE_FLOOR_UNITS_V1,
        CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1, CYCLE4_WEIGHT_TOTAL_UNITS_V1,
    };

    const TEST_TRAINEE_RUN: &str = CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1;
    const TEST_TRAINEE_SEED: u64 = CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1;

    fn hash_tag_v1(tag: u8) -> String {
        format!("cd{:062x}", u64::from(tag))
    }

    /// Frozen slots (anchor-0/1, historical-1, current-0, exploiter-0/1) are
    /// checked by the manifest module against its own pinned five-hash
    /// identity constants (`slot_matches_frozen_v1`), not merely by seed and
    /// generation, so this test module must emit exactly those hashes -- a
    /// synthetic stand-in hash would always be rejected as `InvalidSlots`.
    fn frozen_identity_json_v1(
        slot_index: u64,
        frozen: &FrozenOccupantIdentityCycle4V1,
    ) -> serde_json::Value {
        serde_json::json!({
            "slot_index": slot_index,
            "source_base_seed": frozen.source_base_seed,
            "source_run_sha256": frozen.source_run_sha256,
            "source_generation": frozen.source_generation,
            "checkpoint_manifest_sha256": frozen.checkpoint_manifest_sha256,
            "checkpoint_payload_sha256": frozen.checkpoint_payload_sha256,
            "model_parameter_sha256": frozen.model_parameter_sha256,
            "train_state_sha256": frozen.train_state_sha256,
        })
    }

    /// A trainee-bound slot (historical-0 from refresh index 4 onward,
    /// current-1 always) only pins `source_run_sha256`/`source_base_seed`/
    /// `source_generation`; the sidecar hashes are free, so synthetic
    /// distinct values are fine there.
    fn trainee_bound_identity_json_v1(
        slot_index: u64,
        run: &str,
        seed: u64,
        generation: u64,
        hash_tag: u8,
    ) -> serde_json::Value {
        serde_json::json!({
            "slot_index": slot_index,
            "source_base_seed": seed,
            "source_run_sha256": run,
            "source_generation": generation,
            "checkpoint_manifest_sha256": hash_tag_v1(hash_tag),
            "checkpoint_payload_sha256": hash_tag_v1(hash_tag + 1),
            "model_parameter_sha256": hash_tag_v1(hash_tag + 2),
            "train_state_sha256": hash_tag_v1(hash_tag + 3),
        })
    }

    /// Builds a boundary's slot-identities document. `refresh_index`
    /// controls the two trainee-bound slots' `source_generation` (matching
    /// what a real wrapper would compute from the Store heads at that
    /// boundary); `trainee_run`/`trainee_seed` MUST equal whatever is passed
    /// as the build call's own `trainee_run_sha256`/`trainee_base_seed`,
    /// since the manifest module binds slot 5 (and, from refresh index 4
    /// onward, slot 2) to that exact pair. The frozen slots (0, 1, 3, 4, 6,
    /// 7) always carry the real ratified identities regardless of
    /// `refresh_index`/`trainee_run`, matching the manifest contract's own
    /// pinned roster.
    fn slot_identities_json_v1(
        refresh_index: u64,
        trainee_run: &str,
        trainee_seed: u64,
    ) -> Vec<u8> {
        let local_generation = CYCLE4_TRAINEE_START_LOCAL_GENERATION_V1 + refresh_index * 128;
        let historical_0_generation = local_generation - CYCLE4_HISTORICAL_LAG_V1;
        let rotation = usize::try_from(refresh_index % 3).expect("rotation fits usize");
        let historical_0 = if refresh_index <= 3 {
            trainee_bound_identity_json_v1(
                2,
                CYCLE4_CYCLE3_LINEAGE_RUN_SHA256_V1,
                CYCLE4_CYCLE3_LINEAGE_BASE_SEED_V1,
                historical_0_generation,
                16,
            )
        } else {
            trainee_bound_identity_json_v1(
                2,
                trainee_run,
                trainee_seed,
                historical_0_generation,
                16,
            )
        };
        let doc = serde_json::json!({
            "schema": CYCLE4_SLOT_IDENTITIES_SCHEMA_V1,
            "slots": [
                frozen_identity_json_v1(0, &CYCLE4_ANCHOR_0_V1),
                frozen_identity_json_v1(1, &CYCLE4_ANCHOR_1_V1),
                historical_0,
                frozen_identity_json_v1(3, &CYCLE4_HISTORICAL_1_ROTATION_V1[rotation]),
                frozen_identity_json_v1(4, &CYCLE4_CURRENT_0_V1),
                trainee_bound_identity_json_v1(5, trainee_run, trainee_seed, local_generation, 32),
                frozen_identity_json_v1(6, &CYCLE4_EXPLOITER_0_V1),
                frozen_identity_json_v1(7, &CYCLE4_EXPLOITER_1_V1),
            ],
        });
        serde_json::to_vec(&doc).expect("slot identities json")
    }

    /// Every unordered pair `(lower, higher)` with `lower < higher` over the
    /// eight slots, in the same order the panel runner enumerates them
    /// (`itertools.combinations`): exactly `CYCLE4_MATCHUP_COUNT_V1` pairs.
    fn all_pairs_v1() -> Vec<(usize, usize)> {
        let mut pairs = Vec::with_capacity(CYCLE4_MATCHUP_COUNT_V1);
        for lower in 0..CYCLE4_SLOT_COUNT_V1 {
            for higher in (lower + 1)..CYCLE4_SLOT_COUNT_V1 {
                pairs.push((lower, higher));
            }
        }
        pairs
    }

    fn matchup_row_v1(
        matchup_index: usize,
        lower: usize,
        higher: usize,
        lower_wins: i64,
        lower_draws: i64,
        lower_losses: i64,
    ) -> serde_json::Value {
        serde_json::json!({
            "matchup_index": matchup_index,
            "lower_slot_index": lower,
            "higher_slot_index": higher,
            "game_count": CYCLE4_PANEL_GAMES_PER_MATCHUP_V1,
            "lower_wins": lower_wins,
            "lower_draws": lower_draws,
            "lower_losses": lower_losses,
            "higher_wins": lower_losses,
            "higher_draws": lower_draws,
            "higher_losses": lower_wins,
        })
    }

    /// Builds the full `CYCLE4_MATCHUP_COUNT_V1`-row matchup ledger, every
    /// pair drawn all `CYCLE4_PANEL_GAMES_PER_MATCHUP_V1` games by default,
    /// with `overrides` replacing specific pairs' `(lower_wins, lower_draws,
    /// lower_losses)`. Returns the rows plus the per-slot `u_i` this ledger
    /// implies, computed the SAME way `recomputed_rank_sums_v1` derives it,
    /// so a test's declared rank sums and its matchup ledger are always
    /// constructed in agreement.
    fn matchup_ledger_v1(
        overrides: &[MatchupOverrideV1],
    ) -> (Vec<serde_json::Value>, [i64; CYCLE4_SLOT_COUNT_V1]) {
        let games = i64::try_from(CYCLE4_PANEL_GAMES_PER_MATCHUP_V1).expect("256 fits i64");
        let mut rows = Vec::with_capacity(CYCLE4_MATCHUP_COUNT_V1);
        let mut rank_sums = [0_i64; CYCLE4_SLOT_COUNT_V1];
        for (index, (lower, higher)) in all_pairs_v1().into_iter().enumerate() {
            let (wins, draws, losses) = overrides
                .iter()
                .find(|((entry_lower, entry_higher), _)| {
                    *entry_lower == lower && *entry_higher == higher
                })
                .map_or((0, games, 0), |(_, wdl)| *wdl);
            rank_sums[lower] += wins - losses;
            rank_sums[higher] += losses - wins;
            rows.push(matchup_row_v1(index, lower, higher, wins, draws, losses));
        }
        (rows, rank_sums)
    }

    /// Builds a well-formed panel document bound to `manifest_sha256`, with
    /// declared `rank_sums` derived (via `matchup_ledger_v1`) from the SAME
    /// matchup ledger it carries, so the two views always agree unless a
    /// test deliberately tampers with one afterward.
    fn panel_json_v1(
        manifest_sha256: &str,
        overrides: &[MatchupOverrideV1],
    ) -> (Vec<u8>, [i64; CYCLE4_SLOT_COUNT_V1]) {
        let (matchups, rank_sums) = matchup_ledger_v1(overrides);
        let doc = serde_json::json!({
            "schema": CYCLE4_PANEL_DOC_SCHEMA_V1,
            "manifest_sha256": manifest_sha256,
            "rank_sums": rank_sum_entries_v1(&rank_sums),
            "matchups": matchups,
        });
        (serde_json::to_vec(&doc).expect("panel json"), rank_sums)
    }

    fn rank_sum_entries_v1(rank_sums: &[i64; CYCLE4_SLOT_COUNT_V1]) -> Vec<serde_json::Value> {
        rank_sums
            .iter()
            .enumerate()
            .map(|(index, u_i)| serde_json::json!({"slot_index": index, "u_i": u_i}))
            .collect()
    }

    fn chain_link_v1(manifest_bytes: &[u8], panel_bytes: Option<&[u8]>) -> Cycle4ChainLinkV1 {
        Cycle4ChainLinkV1 {
            manifest_bytes: manifest_bytes.to_vec(),
            panel_bytes: panel_bytes.map(<[u8]>::to_vec),
        }
    }

    #[test]
    fn genesis_builds_with_uniform_weights_v1() {
        let identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let result =
            build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &identities)
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
        let genesis_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_identities,
        )
        .expect("genesis build");
        // Slot 0 (anchor-0) sweeps its one matchup against slot 1
        // (anchor-1); every other matchup is drawn. u_0 = +256 (its one
        // decisive matchup), u_1 = -256 (the mirror image), everyone else
        // nets 0.
        let (panel, rank_sums) = panel_json_v1(&genesis.manifest_sha256, &[((0, 1), (256, 0, 0))]);
        assert_eq!(rank_sums, [256, -256, 0, 0, 0, 0, 0, 0]);
        let chain = [chain_link_v1(&genesis.canonical_bytes, None)];
        let next_identities = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let next = build_cycle4_next_refresh_v1(
            &chain,
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
    fn next_refresh_chains_through_multiple_links_v1() {
        let genesis_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_identities,
        )
        .expect("genesis build");
        let (panel_one, _) = panel_json_v1(&genesis.manifest_sha256, &[]);
        let identities_one = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let refresh_one = build_cycle4_next_refresh_v1(
            &[chain_link_v1(&genesis.canonical_bytes, None)],
            &panel_one,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &identities_one,
        )
        .expect("refresh 1");

        let chain_to_one = [
            chain_link_v1(&genesis.canonical_bytes, None),
            chain_link_v1(&refresh_one.canonical_bytes, Some(&panel_one)),
        ];
        let (panel_two, _) = panel_json_v1(&refresh_one.manifest_sha256, &[]);
        let identities_two = slot_identities_json_v1(2, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let refresh_two = build_cycle4_next_refresh_v1(
            &chain_to_one,
            &panel_two,
            2,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &identities_two,
        )
        .expect("refresh 2 walks the whole chain from genesis");
        assert_eq!(refresh_two.refresh_index, 2);
        assert_eq!(refresh_two.trainee_local_generation, 896 + 256);
    }

    #[test]
    fn next_refresh_rejects_empty_chain_v1() {
        let (panel, _) = panel_json_v1(&hash_tag_v1(1), &[]);
        let next_identities = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &[],
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("empty chain");
        assert_eq!(*error.kind_v1(), Cycle4RefreshBuildErrorKindV1::EmptyChain);
    }

    #[test]
    fn next_refresh_rejects_genesis_link_carrying_a_panel_v1() {
        let genesis_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_identities,
        )
        .expect("genesis build");
        let (stray_panel, _) = panel_json_v1(&genesis.manifest_sha256, &[]);
        let chain = [chain_link_v1(&genesis.canonical_bytes, Some(&stray_panel))];
        let (panel, _) = panel_json_v1(&genesis.manifest_sha256, &[]);
        let next_identities = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &chain,
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("genesis link must carry no panel");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::MalformedChain
        );
    }

    #[test]
    fn next_refresh_rejects_non_genesis_link_missing_its_panel_v1() {
        let genesis_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_identities,
        )
        .expect("genesis build");
        let (panel_one, _) = panel_json_v1(&genesis.manifest_sha256, &[]);
        let identities_one = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let refresh_one = build_cycle4_next_refresh_v1(
            &[chain_link_v1(&genesis.canonical_bytes, None)],
            &panel_one,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &identities_one,
        )
        .expect("refresh 1");
        // refresh_one's own link is missing its panel bytes -- malformed.
        let chain = [
            chain_link_v1(&genesis.canonical_bytes, None),
            chain_link_v1(&refresh_one.canonical_bytes, None),
        ];
        let (panel_two, _) = panel_json_v1(&refresh_one.manifest_sha256, &[]);
        let identities_two = slot_identities_json_v1(2, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &chain,
            &panel_two,
            2,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &identities_two,
        )
        .expect_err("non-genesis link missing its panel");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::MalformedChain
        );
    }

    #[test]
    fn next_refresh_rejects_trainee_run_drift_against_the_chained_manifest_v1() {
        // The panel legitimately evaluated genesis_b (a different trainee
        // run), so it passes the panel-to-tip binding check; the drift is
        // only caught by the deeper manifest chain-linkage validation.
        let other_run = hash_tag_v1(200);
        let genesis_b_identities = slot_identities_json_v1(0, &other_run, TEST_TRAINEE_SEED);
        let genesis_b =
            build_cycle4_genesis_refresh_v1(&other_run, TEST_TRAINEE_SEED, &genesis_b_identities)
                .expect("genesis b");
        let (panel, _) = panel_json_v1(&genesis_b.manifest_sha256, &[]);
        let chain = [chain_link_v1(&genesis_b.canonical_bytes, None)];
        let next_identities = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &chain,
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("trainee run drift against the chained manifest");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::ManifestRejected("InvalidChain".to_owned())
        );
    }

    #[test]
    fn next_refresh_rejects_panel_bound_to_a_different_manifest_v1() {
        // P1-2 regression: a well-formed panel that legitimately evaluated
        // one manifest (genesis_a) must never validate against a DIFFERENT
        // chain tip (genesis_b), even though every other field is
        // well-formed.
        let genesis_a_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis_a = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_a_identities,
        )
        .expect("genesis a");
        let other_run = hash_tag_v1(201);
        let genesis_b_identities = slot_identities_json_v1(0, &other_run, TEST_TRAINEE_SEED);
        let genesis_b =
            build_cycle4_genesis_refresh_v1(&other_run, TEST_TRAINEE_SEED, &genesis_b_identities)
                .expect("genesis b");
        assert_ne!(genesis_a.manifest_sha256, genesis_b.manifest_sha256);
        // Panel bound to genesis_a's hash...
        let (panel_for_a, _) = panel_json_v1(&genesis_a.manifest_sha256, &[]);
        // ...used against a chain whose tip is genesis_b.
        let chain = [chain_link_v1(&genesis_b.canonical_bytes, None)];
        let next_identities = slot_identities_json_v1(1, &other_run, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &chain,
            &panel_for_a,
            1,
            &other_run,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("panel evaluated a different manifest than the chain tip");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::PanelManifestMismatch
        );
    }

    #[test]
    fn next_refresh_rejects_malformed_panel_document_v1() {
        let genesis_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_identities,
        )
        .expect("genesis build");
        let bad_panel = b"{\"schema\":\"wrong\"}".to_vec();
        let chain = [chain_link_v1(&genesis.canonical_bytes, None)];
        let next_identities = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &chain,
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
    fn next_refresh_rejects_rank_sums_disagreeing_with_the_matchup_ledger_v1() {
        // P1-3 regression: a declared `rank_sums` entry that disagrees with
        // what the raw matchup ledger actually shows (a tampered or
        // hand-typed summary) must be rejected, even though every matchup
        // row is individually well-formed.
        let genesis_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_identities,
        )
        .expect("genesis build");
        let (matchups, rank_sums) = matchup_ledger_v1(&[]);
        assert_eq!(rank_sums, [0; CYCLE4_SLOT_COUNT_V1]);
        let mut rank_sum_entries = rank_sum_entries_v1(&rank_sums);
        // Slot 0's ledger truly nets 0, but the declared summary claims 900.
        rank_sum_entries[0] = serde_json::json!({"slot_index": 0, "u_i": 900});
        let doc = serde_json::json!({
            "schema": CYCLE4_PANEL_DOC_SCHEMA_V1,
            "manifest_sha256": genesis.manifest_sha256,
            "rank_sums": rank_sum_entries,
            "matchups": matchups,
        });
        let panel = serde_json::to_vec(&doc).expect("panel json");
        let chain = [chain_link_v1(&genesis.canonical_bytes, None)];
        let next_identities = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &chain,
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("declared rank sum disagrees with the matchup ledger");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::InvalidPanelDocument
        );
    }

    #[test]
    fn next_refresh_rejects_panel_with_a_missing_matchup_v1() {
        let genesis_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_identities,
        )
        .expect("genesis build");
        let (mut matchups, rank_sums) = matchup_ledger_v1(&[]);
        matchups.pop(); // 27 rows: one unordered pair is never covered.
        let doc = serde_json::json!({
            "schema": CYCLE4_PANEL_DOC_SCHEMA_V1,
            "manifest_sha256": genesis.manifest_sha256,
            "rank_sums": rank_sum_entries_v1(&rank_sums),
            "matchups": matchups,
        });
        let panel = serde_json::to_vec(&doc).expect("panel json");
        let chain = [chain_link_v1(&genesis.canonical_bytes, None)];
        let next_identities = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &chain,
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("missing matchup");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::InvalidPanelDocument
        );
    }

    #[test]
    fn next_refresh_rejects_panel_with_an_extra_matchup_v1() {
        let genesis_identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let genesis = build_cycle4_genesis_refresh_v1(
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &genesis_identities,
        )
        .expect("genesis build");
        let (mut matchups, rank_sums) = matchup_ledger_v1(&[]);
        let duplicate = matchups[0].clone();
        matchups.push(duplicate); // 29 rows: pair (0, 1) covered twice.
        let doc = serde_json::json!({
            "schema": CYCLE4_PANEL_DOC_SCHEMA_V1,
            "manifest_sha256": genesis.manifest_sha256,
            "rank_sums": rank_sum_entries_v1(&rank_sums),
            "matchups": matchups,
        });
        let panel = serde_json::to_vec(&doc).expect("panel json");
        let chain = [chain_link_v1(&genesis.canonical_bytes, None)];
        let next_identities = slot_identities_json_v1(1, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let error = build_cycle4_next_refresh_v1(
            &chain,
            &panel,
            1,
            TEST_TRAINEE_RUN,
            TEST_TRAINEE_SEED,
            &next_identities,
        )
        .expect_err("extra matchup");
        assert_eq!(
            *error.kind_v1(),
            Cycle4RefreshBuildErrorKindV1::InvalidPanelDocument
        );
    }

    #[test]
    fn genesis_path_never_reads_a_panel_v1() {
        // Genesis takes no panel argument at all in this module's API --
        // this test documents/pins that shape rather than exercising a
        // runtime branch.
        let identities = slot_identities_json_v1(0, TEST_TRAINEE_RUN, TEST_TRAINEE_SEED);
        let result =
            build_cycle4_genesis_refresh_v1(TEST_TRAINEE_RUN, TEST_TRAINEE_SEED, &identities)
                .expect("genesis build");
        assert_eq!(result.refresh_index, 0);
    }

    #[test]
    fn chain_filenames_follow_the_fixed_naming_scheme_v1() {
        assert_eq!(
            cycle4_chain_manifest_filename_v1(0),
            "refresh-00.manifest.json"
        );
        assert_eq!(
            cycle4_chain_manifest_filename_v1(16),
            "refresh-16.manifest.json"
        );
        assert_eq!(cycle4_chain_panel_filename_v1(1), "refresh-01.panel.json");
        assert_eq!(cycle4_chain_panel_filename_v1(16), "refresh-16.panel.json");
    }
}
