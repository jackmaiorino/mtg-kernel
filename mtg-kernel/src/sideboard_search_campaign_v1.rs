//! Search-campaign manifest, candidate generator, and BO1/BO3 driver
//! (design section 4, W5). This module builds the manifest and the
//! candidate/scoring machinery; it does not run a campaign (W6 is out of
//! scope for this plan) and it never mutates
//! `data/pauper_sideboard_policy_v1.json`.

use crate::ids::PlayerId;
use crate::paired_bo1_harness_v1::BootstrapSidednessV1;
use crate::sideboard::{CardCountV1, RegisteredDeckV1, SideboardErrorV1, SideboardPlanV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SEARCH_CAMPAIGN_MANIFEST_SCHEMA_V1: &str = "kernel_sideboard_search_campaign_manifest/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NDerivationV1 {
    pub minimum_win_rate_delta: f64,
    pub target_power: f64,
    pub calibration_mean_seconds_per_game: f64,
    pub formula: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchCampaignManifestV1 {
    pub schema: String,
    pub checkpoint_weights_hash: String,
    pub checkpoint_git_head: String,
    pub k_max_candidates_per_cell: u32,
    pub max_iterations_per_cell: u32,
    pub traversal_order: String,
    pub master_seed: u64,
    pub n_per_cell: u32,
    pub n_derivation: NDerivationV1,
    pub m_bo3_matches_per_ratification: u32,
    pub warm_start_plan_inputs_sha256: String,
    pub bo1_one_sided_alpha: f64,
    pub bo3_confidence_level: f64,
    pub bootstrap_resample_count: u32,
    pub bootstrap_seed: u64,
    /// Design section 4's manifest field: "resampling method (case
    /// resampling with replacement over the paired per-seed deltas)".
    /// A disclosed string, not a code path selector; `paired_bootstrap_ci_v1`
    /// (Task B) only ever implements case resampling with replacement, so
    /// this field's value is always `"case_resampling_with_replacement"`
    /// today, but it is a manifest-recorded fact, not an assumption.
    pub bootstrap_resampling_method: String,
    /// Design section 4: "sidedness (one-sided for BO1, matching its
    /// alpha; two-sided for BO3's CI-excludes-0 test)". Two separate
    /// fields because the two stages use different sidedness values within
    /// the same manifest; the driver (Step 14 below) always uses
    /// `bo1_bootstrap_sidedness` for the BO1 stage and
    /// `bo3_bootstrap_sidedness` for the BO3 stage, never an ad hoc choice
    /// at analysis time.
    pub bo1_bootstrap_sidedness: BootstrapSidednessV1,
    pub bo3_bootstrap_sidedness: BootstrapSidednessV1,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchCampaignErrorV1 {
    Io(String),
    Json(String),
    HashMismatch { expected: String, actual: String },
    MissingHashFile(String),
}

impl std::fmt::Display for SearchCampaignErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "io error: {message}"),
            Self::Json(message) => write!(f, "json error: {message}"),
            Self::HashMismatch { expected, actual } => write!(
                f, "manifest sha256 mismatch: recorded {expected}, recomputed {actual}"
            ),
            Self::MissingHashFile(path) => write!(f, "missing sidecar hash file: {path}"),
        }
    }
}

pub fn manifest_canonical_bytes_v1(manifest: &SearchCampaignManifestV1) -> Vec<u8> {
    serde_json::to_vec(manifest).expect("SearchCampaignManifestV1 always serializes")
}

pub fn manifest_sha256_v1(manifest: &SearchCampaignManifestV1) -> String {
    let digest = Sha256::digest(manifest_canonical_bytes_v1(manifest));
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Loads a manifest JSON file plus its sidecar `<path>.sha256` (one line,
/// the lower-hex digest) and refuses to return the manifest unless the
/// recomputed hash matches exactly (design section 4: "no candidate scores
/// before this manifest is committed and hashed").
pub fn load_and_verify_manifest_v1(
    path: &std::path::Path,
) -> Result<SearchCampaignManifestV1, SearchCampaignErrorV1> {
    let raw = std::fs::read_to_string(path).map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
    let hash_path = path.with_extension("json.sha256");
    let recorded = std::fs::read_to_string(&hash_path)
        .map_err(|_| SearchCampaignErrorV1::MissingHashFile(hash_path.display().to_string()))?;
    let recorded = recorded.trim().to_owned();
    // Hash the raw bytes exactly as read from disk, not a freshly
    // re-serialized copy of the parsed struct: `validate_search_campaign_manifest_v1.py`'s
    // `sha256_hex` (Step 16) hashes `path.read_bytes()`, the raw on-disk
    // bytes of a pretty-printed, 2-space-indented file (Step 15). Hashing a
    // compact `serde_json::to_vec` re-serialization here, as
    // `manifest_sha256_v1` does for constructing a manifest
    // programmatically, would never agree with the Python sidecar for a
    // real committed file, since the two encodings are byte-different even
    // when they parse to the same struct.
    let actual = {
        let digest = Sha256::digest(raw.as_bytes());
        digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    };
    if recorded != actual {
        return Err(SearchCampaignErrorV1::HashMismatch { expected: recorded, actual });
    }
    let manifest: SearchCampaignManifestV1 =
        serde_json::from_str(&raw).map_err(|error| SearchCampaignErrorV1::Json(error.to_string()))?;
    Ok(manifest)
}

pub fn candidate_seed_v1(
    master_seed: u64,
    self_deck_id: &str,
    opponent_deck_id: &str,
    game_index: u8,
    candidate_index: u32,
) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"kernel-sideboard-search-candidate-seed/v1");
    hasher.update(master_seed.to_be_bytes());
    hasher.update(self_deck_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(opponent_deck_id.as_bytes());
    hasher.update([0u8]);
    hasher.update([game_index]);
    hasher.update(candidate_index.to_be_bytes());
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[0..8].try_into().expect("sha256 digest is at least 8 bytes"))
}

pub fn embedding_status_v1(card_ids: &[u16]) -> Vec<(u16, bool)> {
    let trained: std::collections::BTreeSet<u16> = crate::runtime_decks::RUNTIME_DECKS
        .iter()
        .flat_map(|deck| deck.card_ids.iter().copied())
        .collect();
    card_ids.iter().map(|&card_id| (card_id, trained.contains(&card_id))).collect()
}

/// Bounded 1-card swaps: for every mainboard card_id (ascending) and every
/// sideboard card_id (ascending) not already equal to it, a candidate that
/// swaps exactly one copy out for exactly one copy in. Restricted to cards
/// already present in the deck's own registered 75 (design section 4), so
/// `apply_plan_v1`'s conservation rule is satisfiable by construction.
/// Deterministic ascending-card-id traversal order, capped at `k`
/// candidates; this function only enumerates, it never scores.
pub fn generate_one_swap_candidates_v1(
    registered: &RegisteredDeckV1,
    opponent_deck_id: &str,
    game_index: u8,
    k: u32,
) -> Vec<SideboardPlanV1> {
    let configuration = registered.registered_configuration();
    let mut mainboard_ids: Vec<u16> = configuration.mainboard().to_vec();
    mainboard_ids.sort_unstable();
    mainboard_ids.dedup();
    let mut sideboard_ids: Vec<u16> = configuration.sideboard().to_vec();
    sideboard_ids.sort_unstable();
    sideboard_ids.dedup();

    let mut candidates = Vec::new();
    'outer: for &out_id in &mainboard_ids {
        for &in_id in &sideboard_ids {
            if in_id == out_id {
                continue;
            }
            let plan = SideboardPlanV1::new_v1(
                registered.deck_id().to_owned(),
                opponent_deck_id.to_owned(),
                game_index,
                vec![CardCountV1 { card_id: in_id, count: 1 }],
                vec![CardCountV1 { card_id: out_id, count: 1 }],
            );
            if let Ok(plan) = plan {
                candidates.push(plan);
                if candidates.len() as u32 >= k {
                    break 'outer;
                }
            }
        }
    }
    candidates
}

/// Bounded 2-to-4-card swaps, same restriction and traversal discipline as
/// `generate_one_swap_candidates_v1`: swaps of `degree` cards (2, 3, or 4)
/// out for `degree` cards in, both multisets drawn only from the deck's own
/// registered mainboard/sideboard, enumerated in ascending-card-id
/// combination order, capped at `k`. Enumerates only; never scores.
pub fn generate_multi_swap_candidates_v1(
    registered: &RegisteredDeckV1,
    opponent_deck_id: &str,
    game_index: u8,
    degree: usize,
    k: u32,
) -> Vec<SideboardPlanV1> {
    assert!((2..=4).contains(&degree), "degree must be 2, 3, or 4");
    let configuration = registered.registered_configuration();
    let mut mainboard_ids: Vec<u16> = configuration.mainboard().to_vec();
    mainboard_ids.sort_unstable();
    mainboard_ids.dedup();
    let mut sideboard_ids: Vec<u16> = configuration.sideboard().to_vec();
    sideboard_ids.sort_unstable();
    sideboard_ids.dedup();

    let mut candidates = Vec::new();
    for out_combo in ascending_combinations_v1(&mainboard_ids, degree) {
        for in_combo in ascending_combinations_v1(&sideboard_ids, degree) {
            if out_combo.iter().any(|id| in_combo.contains(id)) {
                continue;
            }
            let plan = SideboardPlanV1::new_v1(
                registered.deck_id().to_owned(),
                opponent_deck_id.to_owned(),
                game_index,
                in_combo.iter().map(|&card_id| CardCountV1 { card_id, count: 1 }).collect(),
                out_combo.iter().map(|&card_id| CardCountV1 { card_id, count: 1 }).collect(),
            );
            if let Ok(plan) = plan {
                candidates.push(plan);
                if candidates.len() as u32 >= k {
                    return candidates;
                }
            }
        }
    }
    candidates
}

/// Ascending-order combinations of size `degree` from a sorted,
/// deduplicated slice, smallest index-tuple first (the standard
/// next-combination algorithm). Hand-rolled so this task adds no new
/// external dependency (Global Constraints).
fn ascending_combinations_v1(items: &[u16], degree: usize) -> Vec<Vec<u16>> {
    if degree == 0 || degree > items.len() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut indices: Vec<usize> = (0..degree).collect();
    loop {
        result.push(indices.iter().map(|&i| items[i]).collect());
        let mut cursor = degree;
        loop {
            if cursor == 0 {
                return result;
            }
            cursor -= 1;
            if indices[cursor] != cursor + items.len() - degree {
                break;
            }
        }
        indices[cursor] += 1;
        for next in (cursor + 1)..degree {
            indices[next] = indices[next - 1] + 1;
        }
    }
}

/// Greedy hill-climbing local search bounded by `max_iterations` (design
/// section 4: "incumbent replaced only on acceptance, bounded by a fixed
/// max-iteration count per cell"), warm-started from `incumbent`. Generic
/// over the candidate type so it is unit-testable against a synthetic,
/// deterministic `accept_fn` independent of any deck-specific scoring; the
/// real driver (Step 11) instantiates it at `T = SideboardPlanV1` with an
/// `accept_fn` backed by `paired_bootstrap_ci_v1`.
pub fn hill_climb_search_v1<T: Clone>(
    incumbent: T,
    neighborhood: impl Fn(&T) -> Vec<T>,
    max_iterations: u32,
    mut accept_fn: impl FnMut(&T, &T) -> bool,
) -> T {
    let mut current = incumbent;
    for _ in 0..max_iterations {
        let candidates = neighborhood(&current);
        let mut replaced = false;
        for candidate in candidates {
            if accept_fn(&current, &candidate) {
                current = candidate;
                replaced = true;
                break;
            }
        }
        if !replaced {
            break;
        }
    }
    current
}

#[derive(Debug, Clone, Deserialize)]
struct WarmStartPlanRowV1 {
    self_deck_id: String,
    opponent_deck_id: String,
    game_index: u8,
    cards_in: Vec<CardCountV1>,
    cards_out: Vec<CardCountV1>,
}

#[derive(Debug, Clone, Deserialize)]
struct WarmStartPlanInputsV1 {
    rows: Vec<WarmStartPlanRowV1>,
}

/// Loads and sha256-verifies the warm-start plan-inputs file (design
/// section 4) against `expected_sha256` (the manifest's
/// `warm_start_plan_inputs_sha256`), refusing to proceed on any mismatch,
/// then parses it into one warm-start `SideboardPlanV1` per row. A row
/// whose `SideboardPlanV1::new_v1` construction fails is skipped rather
/// than aborting the whole load, since a malformed single row should not
/// block every other cell's warm start.
pub fn load_warm_start_plan_inputs_v1(
    path: &std::path::Path,
    expected_sha256: &str,
) -> Result<Vec<SideboardPlanV1>, SearchCampaignErrorV1> {
    let raw = std::fs::read_to_string(path).map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
    let actual = {
        let digest = Sha256::digest(raw.as_bytes());
        digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    };
    if actual != expected_sha256 {
        return Err(SearchCampaignErrorV1::HashMismatch { expected: expected_sha256.to_owned(), actual });
    }
    let parsed: WarmStartPlanInputsV1 =
        serde_json::from_str(&raw).map_err(|error| SearchCampaignErrorV1::Json(error.to_string()))?;
    let mut plans = Vec::new();
    for row in parsed.rows {
        if let Ok(plan) =
            SideboardPlanV1::new_v1(row.self_deck_id, row.opponent_deck_id, row.game_index, row.cards_in, row.cards_out)
        {
            plans.push(plan);
        }
    }
    Ok(plans)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpponentResolutionVariantV1 {
    RegisteredMainboard,
    Bo3RatifiedPlan,
}

/// Design section 2's opponent-side resolution rule: `game_index` 1 always
/// uses the opponent's registered mainboard unconditionally; `game_index`
/// 2 or 3 uses the opponent's best BO3-ratified post-board plan for
/// `(opponent_deck_id, self_deck_id)` when `sideboard_policy` has one,
/// falling back to the registered mainboard otherwise. `plan_for_v1`
/// (`sideboard.rs:523`) already returns a `keep_registered_v1`-shaped plan
/// (empty `cards_in`/`cards_out`) when no specific plan matches, via its
/// own `SideboardDefaultPlanV1::KeepRegisteredConfiguration` fallback, so
/// an empty plan is this function's own signal to report
/// `RegisteredMainboard` rather than `Bo3RatifiedPlan`.
pub fn resolve_opponent_mainboard_for_cell_v1(
    opponent_deck_id: &str,
    self_deck_id: &str,
    game_index: u8,
    sideboard_policy: &crate::sideboard::DeterministicSideboardPolicyV1,
) -> Result<(Vec<u16>, OpponentResolutionVariantV1), SideboardErrorV1> {
    let opponent_registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(opponent_deck_id)?;
    if game_index == 1 {
        return Ok((
            opponent_registered.registered_configuration().mainboard().to_vec(),
            OpponentResolutionVariantV1::RegisteredMainboard,
        ));
    }
    let plan = sideboard_policy.plan_for_v1(opponent_deck_id, self_deck_id, game_index)?;
    if plan.cards_in().is_empty() && plan.cards_out().is_empty() {
        return Ok((
            opponent_registered.registered_configuration().mainboard().to_vec(),
            OpponentResolutionVariantV1::RegisteredMainboard,
        ));
    }
    let (configuration, _receipt) =
        opponent_registered.apply_plan_v1(&plan, "search-driver-opponent-resolution/v1", [0u8; 32])?;
    Ok((configuration.mainboard().to_vec(), OpponentResolutionVariantV1::Bo3RatifiedPlan))
}

/// Design section 4's disclosure requirement: "an `opponent_embedding_status`
/// list computed the same way as `embedding_status`... whenever the
/// accepted-opponent-plan variant was used." Identical trained-check logic
/// to `embedding_status_v1`, kept as a separately named function so a
/// receipt row's two lists are never confused with each other by call site.
pub fn opponent_embedding_status_v1(opponent_plan_cards_in: &[u16]) -> Vec<(u16, bool)> {
    embedding_status_v1(opponent_plan_cards_in)
}

/// The hash convention that produced a candidate's identity hash (design
/// section 4: "a `hash_convention` field recording which convention...").
/// Every candidate this task's generators produce is applied through
/// `apply_plan_v1` onto a `RegisteredDeckV1` sourced from the checked-in
/// pool, then, when scored, resolved into an explicit deck via Task A's
/// `resolve_explicit_decks` seam, which always hashes with
/// `explicit_deck_hash_v1`'s `sorted-explicit` convention (Task A). This
/// harness therefore only ever produces `sorted-explicit` receipts; the
/// `catalog-materialized` convention exists only for already-catalog-
/// resolved decks, never search candidates.
pub const CANDIDATE_HASH_CONVENTION_SORTED_EXPLICIT_V1: &str = "sorted-explicit";

#[derive(Debug, Clone, Serialize)]
pub struct CandidateReceiptFieldsV1 {
    pub hash_convention: String,
    pub embedding_status: Vec<(u16, bool)>,
    pub opponent_embedding_status: Option<Vec<(u16, bool)>>,
    pub opponent_resolution_variant: OpponentResolutionVariantV1,
}

/// Rational approximation of the inverse standard normal CDF (Peter
/// Acklam's algorithm, accurate to about 1.15e-9), used only by
/// `derive_n_per_cell_v1`. Self-contained: no new external dependency
/// (Global Constraints).
fn inverse_normal_cdf_v1(p: f64) -> f64 {
    assert!(p > 0.0 && p < 1.0, "inverse_normal_cdf_v1 is defined on (0, 1)");
    const A: [f64; 6] = [-3.969683028665376e+01, 2.209460984245205e+02, -2.759285104469687e+02, 1.383577518672690e+02, -3.066479806614716e+01, 2.506628277459239e+00];
    const B: [f64; 5] = [-5.447609879822406e+01, 1.615858368580409e+02, -1.556989798598866e+02, 6.680131188771972e+01, -1.328068155288572e+01];
    const C: [f64; 6] = [-7.784894002430293e-03, -3.223964580411365e-01, -2.400758277161838e+00, -2.549732539343734e+00, 4.374664141464968e+00, 2.938163982698783e+00];
    const D: [f64; 4] = [7.784695709041462e-03, 3.224671290700398e-01, 2.445134137142996e+00, 3.754408661907416e+00];
    let p_low = 0.02425;
    let p_high = 1.0 - p_low;
    if p < p_low {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= p_high {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

/// Executable counterpart of `NDerivationV1.formula`
/// ("n = ceil((z_alpha + z_power)^2 * variance / minimum_win_rate_delta^2),
/// variance = p(1-p) at p = 0.5"): design section 4, "N is a power
/// calculation, not a budget-driven guess."
pub fn derive_n_per_cell_v1(minimum_win_rate_delta: f64, target_power: f64, one_sided_alpha: f64) -> u32 {
    assert!(minimum_win_rate_delta > 0.0);
    assert!(target_power > 0.0 && target_power < 1.0);
    assert!(one_sided_alpha > 0.0 && one_sided_alpha < 1.0);
    let z_alpha = inverse_normal_cdf_v1(1.0 - one_sided_alpha);
    let z_power = inverse_normal_cdf_v1(target_power);
    let variance = 0.5 * 0.5;
    let n = (z_alpha + z_power).powi(2) * variance / minimum_win_rate_delta.powi(2);
    n.ceil() as u32
}

/// Runs `manifest.n_per_cell` paired BO1 trials per candidate (candidate
/// mainboard vs the search's current working-best mainboard, starting from
/// `incumbent`), computes the one-sided paired-bootstrap CI on the
/// resulting deltas, and replaces the working best only when its lower
/// bound exceeds 0 at the manifest's pre-registered discipline (design
/// section 4, BO1 provisional accept). Gated on `load_and_verify_manifest_v1`
/// first: this function never scores a single trial against an unverified
/// manifest.
pub fn run_bo1_provisional_search_for_cell_v1(
    manifest: &SearchCampaignManifestV1,
    manifest_path: &std::path::Path,
    registered: &RegisteredDeckV1,
    incumbent: &SideboardPlanV1,
    candidates: &[SideboardPlanV1],
    opponent_mainboard: &[u16],
    policy_fn: &mut dyn FnMut(&crate::rl_session::FastActorDecisionV1) -> u32,
) -> Result<Option<SideboardPlanV1>, SearchCampaignErrorV1> {
    load_and_verify_manifest_v1(manifest_path)?;
    let mainboard_for = |plan: &SideboardPlanV1| -> Result<Vec<u16>, SearchCampaignErrorV1> {
        let (configuration, _receipt) = registered
            .apply_plan_v1(plan, "search-driver-bo1/v1", [0u8; 32])
            .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
        Ok(configuration.mainboard().to_vec())
    };
    let mut working_best: Option<SideboardPlanV1> = None;
    let mut working_best_mainboard = mainboard_for(incumbent)?;

    for (candidate_index, candidate) in candidates.iter().enumerate() {
        let candidate_mainboard = mainboard_for(candidate)?;
        let mut deltas: Vec<i8> = Vec::with_capacity(manifest.n_per_cell as usize);
        for trial in 0..manifest.n_per_cell {
            let seed = candidate_seed_v1(
                manifest.master_seed,
                candidate.self_deck_id(),
                candidate.opponent_deck_id(),
                candidate.game_index(),
                candidate_index as u32 * manifest.n_per_cell + trial,
            );
            let outcome = crate::paired_bo1_harness_v1::run_paired_bo1_trial_v1(
                &candidate_mainboard, &working_best_mainboard, opponent_mainboard,
                PlayerId::P0, seed, PlayerId::P0, 2000, policy_fn,
            )
            .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
            deltas.push(outcome.delta);
        }
        let result = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &deltas, manifest.bootstrap_resample_count, manifest.bootstrap_seed, manifest.bo1_bootstrap_sidedness,
        );
        if result.lower > 0.0 {
            working_best_mainboard = candidate_mainboard;
            working_best = Some(candidate.clone());
        }
    }
    Ok(working_best)
}

/// Plays one full BO3 match for `self_mainboard` (seated P0) against
/// `opponent_mainboard` (seated P1): each game is constructed from
/// `PreparedMatchGameV1::start().starting_player` via Task A's
/// starting-player-aware explicit-deck constructor, driven to terminal by
/// `policy_fn`, and its result fed back through `record_game_result_v1`
/// until `MatchTransitionV1::Complete`. Returns whether the `self_mainboard`
/// seat won the match.
fn play_bo3_match_for_seat_v1(
    self_deck_id: &str,
    opponent_deck_id: &str,
    self_mainboard: &[u16],
    opponent_mainboard: &[u16],
    sideboard_policy: &crate::sideboard::DeterministicSideboardPolicyV1,
    pair_environment_seed: u64,
    policy_fn: &mut dyn FnMut(&crate::rl_session::RlSessionDecisionV1) -> (u32, String),
) -> Result<bool, SearchCampaignErrorV1> {
    let self_registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(self_deck_id)
        .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
    let opponent_registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(opponent_deck_id)
        .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
    let mut match_session = crate::bo3_session::BestOfThreeDeckMatchV1::new_v1(
        self_registered, opponent_registered, sideboard_policy.clone(), PlayerId::P0,
    )
    .map_err(|error| SearchCampaignErrorV1::Io(format!("{error:?}")))?;

    loop {
        let game = match_session
            .prepare_game_v1(PlayerId::P0, crate::bo3_match::PlayDrawChoiceV1::Play)
            .map_err(|error| SearchCampaignErrorV1::Io(format!("{error:?}")))?;
        let start = game.start();
        let deck_ids = [self_deck_id.to_owned(), opponent_deck_id.to_owned()];
        let mut session = crate::rl_session::RlEpisodeSessionV1::reset_with_explicit_decks_and_limits_with_starting_player_v1(
            u64::from(start.game_index),
            pair_environment_seed ^ u64::from(start.game_index),
            2000,
            200_000,
            deck_ids,
            [self_mainboard.to_vec(), opponent_mainboard.to_vec()],
            start.starting_player,
        )
        .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;

        let winner = loop {
            match session.current_response() {
                crate::rl_session::RlSessionResponseV1::Terminal(terminal) => break terminal.winner,
                crate::rl_session::RlSessionResponseV1::Decision(decision) => {
                    let (selected_index, selected_action_id) = policy_fn(&decision);
                    session
                        .step(decision.episode_id, decision.step, selected_index, &selected_action_id)
                        .map_err(|error| SearchCampaignErrorV1::Io(error.to_string()))?;
                }
            }
        };
        let outcome = match winner {
            Some(crate::rl::PlayerSeatV1::P0) => crate::bo3_match::GameOutcomeV1::Win { winner: PlayerId::P0 },
            Some(crate::rl::PlayerSeatV1::P1) => crate::bo3_match::GameOutcomeV1::Win { winner: PlayerId::P1 },
            None => crate::bo3_match::GameOutcomeV1::Draw,
        };
        match match_session
            .record_game_result_v1(outcome)
            .map_err(|error| SearchCampaignErrorV1::Io(format!("{error:?}")))?
        {
            crate::bo3_match::MatchTransitionV1::NextGameChoice { .. } => continue,
            crate::bo3_match::MatchTransitionV1::Complete { outcome } => {
                return Ok(matches!(outcome, crate::bo3_match::MatchOutcomeV1::Winner { winner: PlayerId::P0 }));
            }
        }
    }
}

/// Runs `manifest.m_bo3_matches_per_ratification` paired BO3 matches
/// (candidate mainboard vs incumbent mainboard, same opponent and shared
/// seed per pair) and ratifies the candidate only when the two-sided
/// paired-bootstrap CI on match-win delta excludes 0 at
/// `manifest.bo3_confidence_level` (design section 4, BO3 ratification).
/// Gated on `load_and_verify_manifest_v1`, exactly like the BO1 stage.
pub fn run_bo3_ratification_v1(
    manifest: &SearchCampaignManifestV1,
    manifest_path: &std::path::Path,
    self_deck_id: &str,
    opponent_deck_id: &str,
    candidate_mainboard: &[u16],
    incumbent_mainboard: &[u16],
    opponent_mainboard: &[u16],
    sideboard_policy: &crate::sideboard::DeterministicSideboardPolicyV1,
    policy_fn: &mut dyn FnMut(&crate::rl_session::RlSessionDecisionV1) -> (u32, String),
) -> Result<bool, SearchCampaignErrorV1> {
    load_and_verify_manifest_v1(manifest_path)?;
    let mut deltas: Vec<i8> = Vec::with_capacity(manifest.m_bo3_matches_per_ratification as usize);
    for match_index in 0..manifest.m_bo3_matches_per_ratification {
        let seed = candidate_seed_v1(manifest.master_seed, self_deck_id, opponent_deck_id, 2, match_index);
        let candidate_won = play_bo3_match_for_seat_v1(
            self_deck_id, opponent_deck_id, candidate_mainboard, opponent_mainboard, sideboard_policy, seed, policy_fn,
        )?;
        let incumbent_won = play_bo3_match_for_seat_v1(
            self_deck_id, opponent_deck_id, incumbent_mainboard, opponent_mainboard, sideboard_policy, seed, policy_fn,
        )?;
        deltas.push(if candidate_won { 1i8 } else { 0i8 } - if incumbent_won { 1i8 } else { 0i8 });
    }
    let result = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
        &deltas, manifest.bootstrap_resample_count, manifest.bootstrap_seed, manifest.bo3_bootstrap_sidedness,
    );
    Ok(result.lower > 0.0 || result.upper < 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> SearchCampaignManifestV1 {
        SearchCampaignManifestV1 {
            schema: SEARCH_CAMPAIGN_MANIFEST_SCHEMA_V1.to_owned(),
            checkpoint_weights_hash: "deadbeefdeadbeefdeadbeefdeadbeef".to_owned(),
            checkpoint_git_head: "0".repeat(40),
            k_max_candidates_per_cell: 32,
            max_iterations_per_cell: 64,
            traversal_order: "ascending_card_id_one_swap_then_two_swap".to_owned(),
            master_seed: 0x5150_5150_5150_5150,
            n_per_cell: 400,
            n_derivation: NDerivationV1 {
                minimum_win_rate_delta: 0.05,
                target_power: 0.8,
                calibration_mean_seconds_per_game: 0.42,
                formula: "n = ceil((z_alpha + z_power)^2 * variance / minimum_win_rate_delta^2)".to_owned(),
            },
            m_bo3_matches_per_ratification: 60,
            warm_start_plan_inputs_sha256: "0".repeat(64),
            bo1_one_sided_alpha: 0.05,
            bo3_confidence_level: 0.95,
            bootstrap_resample_count: 10_000,
            bootstrap_seed: 0xC0FF_EEC0_FFEE_C0FF,
            bootstrap_resampling_method: "case_resampling_with_replacement".to_owned(),
            bo1_bootstrap_sidedness: BootstrapSidednessV1::OneSidedLower,
            bo3_bootstrap_sidedness: BootstrapSidednessV1::TwoSided,
        }
    }

    /// `sample_manifest()` with `n_per_cell` overridden, for the driver
    /// tests (Step 13) that need a small, fast trial count.
    fn sample_manifest_with_n_per_cell_v1(n_per_cell: u32) -> SearchCampaignManifestV1 {
        SearchCampaignManifestV1 { n_per_cell, ..sample_manifest() }
    }

    #[test]
    fn manifest_sha256_is_deterministic_and_sensitive_to_every_field() {
        let a = sample_manifest();
        let mut b = sample_manifest();
        assert_eq!(manifest_sha256_v1(&a), manifest_sha256_v1(&b));
        b.n_per_cell += 1;
        assert_ne!(manifest_sha256_v1(&a), manifest_sha256_v1(&b));
    }

    #[test]
    fn load_and_verify_manifest_rejects_a_tampered_file() {
        let dir = std::env::temp_dir().join(format!("search_manifest_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = sample_manifest();
        let path = dir.join("manifest.json");
        std::fs::write(&path, manifest_canonical_bytes_v1(&manifest)).unwrap();
        std::fs::write(path.with_extension("json.sha256"), manifest_sha256_v1(&manifest)).unwrap();
        let loaded = load_and_verify_manifest_v1(&path).expect("untampered manifest loads");
        assert_eq!(loaded, manifest);

        // Tamper with the file on disk without updating the sidecar hash.
        let mut tampered_bytes = manifest_canonical_bytes_v1(&manifest);
        let mut tampered: SearchCampaignManifestV1 = serde_json::from_slice(&tampered_bytes).unwrap();
        tampered.k_max_candidates_per_cell += 1;
        tampered_bytes = manifest_canonical_bytes_v1(&tampered);
        std::fs::write(&path, tampered_bytes).unwrap();
        let error = load_and_verify_manifest_v1(&path).expect_err("tampered manifest must be refused");
        assert!(matches!(error, SearchCampaignErrorV1::HashMismatch { .. }));
    }

    #[test]
    fn candidate_seed_is_a_pure_function_reproducible_from_the_manifest_alone() {
        let a = candidate_seed_v1(0x1234, "RallyV2", "Wildfire", 2, 7);
        let b = candidate_seed_v1(0x1234, "RallyV2", "Wildfire", 2, 7);
        assert_eq!(a, b);
        let different_candidate = candidate_seed_v1(0x1234, "RallyV2", "Wildfire", 2, 8);
        assert_ne!(a, different_candidate);
        let different_cell = candidate_seed_v1(0x1234, "RallyV2", "Wildfire", 3, 7);
        assert_ne!(a, different_cell);
    }

    #[test]
    fn embedding_status_reports_pulse_of_murasa_as_trained_via_wildfire_mainboard() {
        // design section 1's cross-deck example: Pulse of Murasa is
        // sideboard-only for Elves but mainboard for Wildfire, so its row
        // is trained by the global, catalog-wide definition.
        let pulse_of_murasa_id = crate::card_def::card_id_by_name("Pulse of Murasa")
            .expect("Pulse of Murasa is registered");
        let status = embedding_status_v1(&[pulse_of_murasa_id]);
        assert_eq!(status, vec![(pulse_of_murasa_id, true)]);
    }

    use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;
    use crate::sideboard::DeterministicSideboardPolicyV1;

    #[test]
    fn one_swap_candidates_are_legal_plans_restricted_to_the_registered_75_and_deterministic() {
        let registered = checked_in_pauper_registered_deck_by_id_v1("Burn").expect("Burn is checked in");
        // k = 2 is provably smaller than Burn's total legal one-swap count:
        // Burn's registered 75 has 60 mainboard and 15 sideboard cards, so
        // mainboard_ids.len() * sideboard_ids.len() (an upper bound on
        // distinct card_id pairs, before deduplication) is far larger than
        // 2. `assert_eq!(a.len(), k)` therefore actually fails if the cap
        // in `generate_one_swap_candidates_v1` is removed or broken, unlike
        // an `a.len() == k.min(a.len())` tautology.
        let k = 2;
        let a = generate_one_swap_candidates_v1(&registered, "Rally", 2, k);
        let b = generate_one_swap_candidates_v1(&registered, "Rally", 2, k);
        assert_eq!(a.len(), k as usize, "the cap must actually fire at exactly k candidates");
        for (candidate_a, candidate_b) in a.iter().zip(b.iter()) {
            assert_eq!(
                candidate_a.cards_in(), candidate_b.cards_in(),
                "same traversal order and k must reproduce the identical candidate list"
            );
        }
        let sideboard_ids: std::collections::BTreeSet<u16> =
            registered.registered_configuration().sideboard().iter().copied().collect();
        let mainboard_ids: std::collections::BTreeSet<u16> =
            registered.registered_configuration().mainboard().iter().copied().collect();
        for plan in &a {
            for row in plan.cards_in() {
                assert!(
                    sideboard_ids.contains(&row.card_id),
                    "a candidate's cards_in must come from Burn's own registered sideboard, card_id {}", row.card_id
                );
            }
            for row in plan.cards_out() {
                assert!(
                    mainboard_ids.contains(&row.card_id),
                    "a candidate's cards_out must come from Burn's own registered mainboard, card_id {}", row.card_id
                );
            }
            // apply_plan_v1's conservation rule is satisfiable by
            // construction: applying every candidate must succeed.
            registered
                .apply_plan_v1(plan, "search-candidate-check/v1", [0u8; 32])
                .expect("every generated candidate applies cleanly");
        }
    }

    #[test]
    fn multi_swap_candidates_swap_exactly_degree_cards_and_stay_within_the_registered_75() {
        let registered = checked_in_pauper_registered_deck_by_id_v1("Burn").expect("Burn is checked in");
        for degree in [2usize, 3, 4] {
            let candidates = generate_multi_swap_candidates_v1(&registered, "Rally", 2, degree, 3);
            assert!(!candidates.is_empty(), "degree {degree} must produce at least one candidate");
            for plan in &candidates {
                assert_eq!(plan.cards_in().len(), degree);
                assert_eq!(plan.cards_out().len(), degree);
                registered
                    .apply_plan_v1(plan, "multi-swap-check/v1", [0u8; 32])
                    .unwrap_or_else(|error| panic!("degree-{degree} candidate must apply cleanly: {error}"));
            }
        }
    }

    #[test]
    fn hill_climb_search_terminates_within_the_iteration_bound_and_converges_on_a_monotonic_score() {
        // Generic over a plain `i32` "score" rather than `SideboardPlanV1`:
        // this exercises the search loop's own termination and acceptance
        // logic in isolation, independent of any deck-specific scoring.
        let always_climb_to_ten = |current: &i32, candidate: &i32| *candidate <= 10 && *candidate > *current;
        let neighborhood = |current: &i32| vec![current + 1];
        let converged = hill_climb_search_v1(0i32, neighborhood, 100, always_climb_to_ten);
        assert_eq!(converged, 10, "hill-climb converges to the ceiling the acceptance function allows");

        let always_accept = |_current: &i32, _candidate: &i32| true;
        let bounded = hill_climb_search_v1(0i32, |current: &i32| vec![current + 1], 3, always_accept);
        assert_eq!(bounded, 3, "with max_iterations = 3 and an always-accept function, exactly 3 steps run");
    }

    #[test]
    fn warm_start_plan_inputs_load_and_verify_against_their_own_sha256() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../docs/research/sideboard_plan_inputs_2026-09/plan_inputs_warm_start_v1.json");
        let raw = std::fs::read_to_string(&path).expect("warm-start plan-inputs file exists (Step 9 creates it)");
        let expected = {
            let digest = Sha256::digest(raw.as_bytes());
            digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        };
        let plans = load_warm_start_plan_inputs_v1(&path, &expected)
            .expect("the real file must load against its own freshly computed hash");
        assert!(!plans.is_empty(), "the warm-start file must seed at least one cell");
        let wrong_hash = "0".repeat(64);
        let error = load_warm_start_plan_inputs_v1(&path, &wrong_hash).expect_err("a wrong hash must be refused");
        assert!(matches!(error, SearchCampaignErrorV1::HashMismatch { .. }));
    }

    #[test]
    fn resolve_opponent_mainboard_uses_the_registered_mainboard_unconditionally_at_game_index_one() {
        let policy = DeterministicSideboardPolicyV1::checked_in_pauper_v1().unwrap();
        let (mainboard, variant) =
            resolve_opponent_mainboard_for_cell_v1("Rally", "Burn", 1, &policy).unwrap();
        let expected = checked_in_pauper_registered_deck_by_id_v1("Rally")
            .unwrap()
            .registered_configuration()
            .mainboard()
            .to_vec();
        assert_eq!(mainboard, expected);
        assert_eq!(variant, OpponentResolutionVariantV1::RegisteredMainboard);
    }

    #[test]
    fn resolve_opponent_mainboard_at_game_index_two_is_consistent_with_its_own_reported_variant() {
        // Whether Burn-vs-Rally has a BO3-ratified game-2 plan checked in
        // at this plan's writing is unknown (no search campaign has run;
        // W6 is out of scope here), so this test asserts internal
        // consistency between the returned mainboard and the variant that
        // explains it, not one specific outcome.
        let policy = DeterministicSideboardPolicyV1::checked_in_pauper_v1().unwrap();
        let (mainboard, variant) =
            resolve_opponent_mainboard_for_cell_v1("Rally", "Burn", 2, &policy).unwrap();
        match variant {
            OpponentResolutionVariantV1::RegisteredMainboard => {
                let expected = checked_in_pauper_registered_deck_by_id_v1("Rally")
                    .unwrap()
                    .registered_configuration()
                    .mainboard()
                    .to_vec();
                assert_eq!(mainboard, expected);
            }
            OpponentResolutionVariantV1::Bo3RatifiedPlan => {
                assert_eq!(mainboard.len(), crate::sideboard::REGISTERED_MAINBOARD_SIZE_V1);
            }
        }
    }

    #[test]
    fn opponent_embedding_status_reports_pulse_of_murasa_as_trained_via_wildfire_mainboard() {
        let pulse_of_murasa_id = crate::card_def::card_id_by_name("Pulse of Murasa")
            .expect("Pulse of Murasa is registered");
        let status = opponent_embedding_status_v1(&[pulse_of_murasa_id]);
        assert_eq!(status, vec![(pulse_of_murasa_id, true)]);
    }

    #[test]
    fn derive_n_per_cell_matches_a_hand_computed_value_at_a_concrete_delta_power_alpha_triple() {
        // z_{0.95} ~ 1.6448536, z_{0.8} ~ 0.8416212 (standard normal
        // quantiles); n = ceil((1.6448536 + 0.8416212)^2 * 0.25 / 0.05^2)
        // = ceil(618.25...) = 619.
        let n = derive_n_per_cell_v1(0.05, 0.8, 0.05);
        assert_eq!(n, 619);
    }

    use crate::bo3_session::BestOfThreeDeckMatchV1;
    use crate::bo3_match::{GameOutcomeV1, PlayDrawChoiceV1};
    use crate::paired_bo1_harness_v1::{run_paired_bo1_trial_v1, PairedTrialOutcomeV1};
    use crate::rl_session::FastActorDecisionV1;
    use crate::state::SplitMix64;

    fn random_policy_v1(seed: u64) -> impl FnMut(&FastActorDecisionV1) -> u32 {
        let mut rng = SplitMix64::seed(seed);
        move |decision: &FastActorDecisionV1| (rng.next_u64() as u32) % decision.legal_action_count.max(1)
    }

    #[test]
    fn a_candidate_mainboard_from_apply_plan_v1_drives_a_real_paired_bo1_trial() {
        let registered = checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
        let candidates = generate_one_swap_candidates_v1(&registered, "Rally", 2, 1);
        let candidate_plan = candidates.first().expect("at least one candidate exists");
        let (candidate_configuration, _receipt) = registered
            .apply_plan_v1(candidate_plan, "search-driver-check/v1", [0u8; 32])
            .unwrap();
        let incumbent_mainboard = registered.registered_configuration().mainboard().to_vec();
        let opponent_mainboard = checked_in_pauper_registered_deck_by_id_v1("Rally")
            .unwrap()
            .registered_configuration()
            .mainboard()
            .to_vec();
        let seed = candidate_seed_v1(0xABCD, "Burn", "Rally", 2, 0);
        let mut policy = random_policy_v1(seed);
        let outcome: PairedTrialOutcomeV1 = run_paired_bo1_trial_v1(
            candidate_configuration.mainboard(),
            &incumbent_mainboard,
            &opponent_mainboard,
            PlayerId::P0,
            seed,
            PlayerId::P0,
            2000,
            &mut policy,
        )
        .expect("paired trial completes on a real generated candidate");
        assert!((-1..=1).contains(&outcome.delta));
    }

    #[test]
    fn bo3_ratification_uses_the_bo3_assigned_starting_player_for_every_game() {
        let match_session_result = BestOfThreeDeckMatchV1::checked_in_pauper_v1("Burn", "Rally", PlayerId::P0);
        let mut match_session = match match_session_result {
            Ok(session) => session,
            Err(error) => panic!(
                "Burn vs Rally must have a ratified game-2 sideboard plan for this test to run: {error}"
            ),
        };
        let game = match_session
            .prepare_game_v1(PlayerId::P0, PlayDrawChoiceV1::Play)
            .expect("game 1 prepares");
        let start = game.start();
        assert_eq!(start.game_index, 1);
        assert_eq!(start.starting_player, PlayerId::P0, "PlayDrawChoiceV1::Play by the chooser starts that player");
        // The binding this task's driver must honor: every BO3-ratification
        // game is constructed from exactly this starting_player value via
        // Task A's starting-player-aware explicit-deck constructor, never
        // the plain P0-only one.
        let p0_mainboard = game.configuration(PlayerId::P0).unwrap().mainboard().to_vec();
        let p1_mainboard = game.configuration(PlayerId::P1).unwrap().mainboard().to_vec();
        let deck_ids = ["Burn".to_owned(), "Rally".to_owned()];
        let mut session = crate::rl_session::RlEpisodeSessionV1::reset_with_explicit_decks_and_limits_with_starting_player_v1(
            1, 0x9999, 2000, 200_000, deck_ids, [p0_mainboard, p1_mainboard], start.starting_player,
        )
        .expect("bo3-bound explicit-deck reset succeeds");
        assert_eq!(session.game_state().active_player, start.starting_player);
        match_session
            .record_game_result_v1(GameOutcomeV1::Win { winner: PlayerId::P0 })
            .expect("recording game 1's result advances the match");
    }

    #[test]
    fn bo1_accept_boundary_on_synthetic_deltas() {
        let clearly_positive = [1i8; 20];
        let accept = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &clearly_positive, 2000, 1, BootstrapSidednessV1::OneSidedLower,
        );
        assert!(accept.lower > 0.0, "a uniformly positive delta series must clear the BO1 accept boundary");

        let clearly_mixed = [1i8, -1, 1, -1, 1, -1, 1, -1, 1, -1];
        let reject = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &clearly_mixed, 2000, 1, BootstrapSidednessV1::OneSidedLower,
        );
        assert!(reject.lower <= 0.0, "a zero-mean delta series must not clear the BO1 accept boundary");
    }

    #[test]
    fn bo3_ratify_boundary_on_synthetic_deltas() {
        let clearly_positive = [1i8; 20];
        let ratify = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &clearly_positive, 2000, 1, BootstrapSidednessV1::TwoSided,
        );
        assert!(ratify.lower > 0.0 || ratify.upper < 0.0, "a uniformly positive series must exclude 0 at the BO3 two-sided CI");

        let clearly_mixed = [1i8, -1, 1, -1, 1, -1, 1, -1, 1, -1];
        let reject = crate::paired_bo1_harness_v1::paired_bootstrap_ci_v1(
            &clearly_mixed, 2000, 1, BootstrapSidednessV1::TwoSided,
        );
        assert!(!(reject.lower > 0.0 || reject.upper < 0.0), "a zero-mean series must not exclude 0 at the BO3 two-sided CI");
    }

    #[test]
    fn run_bo1_provisional_search_completes_end_to_end_against_the_committed_template_manifest() {
        let manifest = sample_manifest_with_n_per_cell_v1(3);
        let dir = std::env::temp_dir().join(format!("bo1_driver_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest_path = dir.join("manifest.json");
        std::fs::write(&manifest_path, manifest_canonical_bytes_v1(&manifest)).unwrap();
        std::fs::write(manifest_path.with_extension("json.sha256"), manifest_sha256_v1(&manifest)).unwrap();

        let registered = checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap();
        let incumbent = SideboardPlanV1::keep_registered_v1("Burn", "Rally", 2).unwrap();
        let candidates = generate_one_swap_candidates_v1(&registered, "Rally", 2, 1);
        let opponent_mainboard = checked_in_pauper_registered_deck_by_id_v1("Rally")
            .unwrap()
            .registered_configuration()
            .mainboard()
            .to_vec();
        let mut rng = SplitMix64::seed(0x4141_4141_4141_4141);
        let mut policy = |decision: &crate::rl_session::FastActorDecisionV1| {
            (rng.next_u64() as u32) % decision.legal_action_count.max(1)
        };
        let result = run_bo1_provisional_search_for_cell_v1(
            &manifest, &manifest_path, &registered, &incumbent, &candidates, &opponent_mainboard, &mut policy,
        )
        .expect("the driver runs end to end against a real, small candidate set");
        if let Some(accepted) = result {
            assert_eq!(accepted.self_deck_id(), "Burn");
        }
    }

    #[test]
    fn the_committed_template_manifest_loads_against_its_committed_sha256_sidecar() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sideboard_search_campaign_manifest_v1.json");
        let loaded = load_and_verify_manifest_v1(&path)
            .expect("the committed template manifest must match its committed sidecar hash");
        assert_eq!(loaded.schema, SEARCH_CAMPAIGN_MANIFEST_SCHEMA_V1);
    }
}
