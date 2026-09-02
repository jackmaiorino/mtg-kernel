//! Cycle-4 routing selector: sections B, C and D of the ratified section-6
//! mechanical amendment (`LEAD_CYCLE4_SECTION6_MECHANICAL_AMENDMENT_V2.md`).
//!
//! This is the whole CP7-free selector. It reads the two v4 arms' M3
//! eligibility reports (`native_cycle4_m3_audit_v1`) and the M2 common-root
//! panel (`scripts/experiments/population_v2_cycle4_v1/
//! run_m2_common_root_panel_v1.py`), applies sections B and C with no
//! operator judgement anywhere in the path, and publishes the immutable,
//! content-hashed routing record section D requires BEFORE any M1 CP7 byte
//! becomes readable.
//!
//! # Section B, ranking
//!
//! Two arms are SEPARABLE iff the two-sided 95% CI of their paired M2 delta
//! excludes 0 AND `|point| >= 1.0 pp`. Only the three ARM endpoints are
//! ranked; `g896` does not participate. Ties break toward the simpler recipe
//! in the fixed order [`CYCLE4_ARM_ENDPOINT_IDS_V1`].
//!
//! AMBIGUITY RESOLVED HERE, stated rather than silently chosen: "Copeland
//! score over their mutual comparisons" is implemented as the standard
//! Copeland score, pairwise WINS MINUS pairwise LOSSES. The wins-only
//! variant orders differently whenever exactly one of the three pairs is
//! inseparable (A beats B, B beats C, A vs C inseparable gives A and B one
//! win each under wins-only but +1 and 0 under wins-minus-losses), so the
//! choice is load-bearing and is recorded in the routing record's
//! `copeland_convention` field.
//!
//! # Section C, carry
//!
//! Candidates are tested in rank order among ELIGIBLE arms (CONTROL-R is
//! always eligible; a v4 arm is eligible iff its M3 report's verdict is
//! PASS). An arm carries iff, against the frozen `g896` start on M2, the
//! one-sided 95% lower bound of its pooled seat-blind delta is
//! `> -1.0 pp` AND the one-sided 95% lower bound of its P1-stratum delta is
//! `> -2.0 pp`. If none carries, the record says NO CARRY and names the
//! cycle-3 g2048 checkpoint under the CONTROL-R recipe.
//!
//! # The statistics are recomputed, never trusted
//!
//! The panel declares its own comparison table, but this selector never
//! decides on a declared number. It recomputes every delta, standard error,
//! CI bound and one-sided bound from the panel's own per-root outcome table,
//! in `root_index` order with the same plain sequential sums and two-pass
//! sample standard deviation the runner used, and requires BIT equality with
//! what the runner declared. Root scores are multiples of 0.25 and there are
//! at most 1,024 of them, so the sums are exact in f64 and the two
//! implementations agree bit for bit or one of them is wrong; a disagreement
//! is a hard failure, not a tolerance.
//!
//! # The CP7 guard
//!
//! Section D's freeze order cannot be proven from inside a process, but it
//! can be guarded. `--cp7-evidence-root` names the root where an M1 CP7
//! panel's outcome artifacts would land; this selector refuses to run if
//! that root already holds one naming any of the four endpoints. See
//! [`assert_cp7_evidence_absent_v1`] for exactly what it looks at, and what
//! it therefore cannot see.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonNullPolicyV1,
};
use crate::native_cycle4_m3_audit_v1::{
    decode_cycle4_m3_audit_report_v1, f64_bits_hex_v1, Cycle4M3AuditReportV1, RealV1,
    CYCLE4_M3_VERDICT_PASS_V1,
};
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

/// Schema of the M2 panel this selector consumes.
pub const CYCLE4_M2_PANEL_SCHEMA_V1: &str = "mtg-kernel-cycle4-m2-common-root-panel/v1";
/// Schema of the routing record this selector publishes.
pub const CYCLE4_ROUTING_RECORD_SCHEMA_V1: &str = "mtg-kernel-cycle4-routing-record/v1";

/// Amendment section B: "N = 1,024 common roots per pair."
pub const CYCLE4_ROUTING_ROOT_COUNT_V1: u64 = 1_024;
/// Amendment section B: "Two arms are SEPARABLE iff the CI excludes 0 AND
/// |point| >= 1.0 pp."
pub const CYCLE4_SEPARABILITY_MIN_ABS_POINT_PP_V1: f64 = 1.0;
/// Amendment section C (i).
pub const CYCLE4_CARRY_POOLED_LOWER_BOUND_PP_V1: f64 = -1.0;
/// Amendment section C (ii).
pub const CYCLE4_CARRY_P1_LOWER_BOUND_PP_V1: f64 = -2.0;
/// The same decimal literals the M2 runner pins, parsing to the same
/// doubles: the two-sided and one-sided 95% normal quantiles.
pub const CYCLE4_Z_TWO_SIDED_95_V1: f64 = 1.959_963_984_540_054;
pub const CYCLE4_Z_ONE_SIDED_95_V1: f64 = 1.644_853_626_951_472_2;

/// The three ranked arm endpoints, in the amendment's tie-break order:
/// "break ties toward the simpler recipe in the fixed order CONTROL-R, then
/// STATIC-RB, then TREATMENT-RB."
pub const CYCLE4_ARM_ENDPOINT_IDS_V1: [&str; 3] = ["control-r", "static-rb", "treatment-rb"];
/// The frozen start. It is measured on the same roots but never ranked.
pub const CYCLE4_BASELINE_ENDPOINT_ID_V1: &str = "g896";
/// The two arms whose eligibility the M3 gate decides. CONTROL-R is v3 and
/// "is always eligible".
pub const CYCLE4_V4_ARM_ENDPOINT_IDS_V1: [&str; 2] = ["static-rb", "treatment-rb"];

const CYCLE4_LOSS_IDENTITY_V3_V1: &str = "terminal_reinforce_value/v3";
const CYCLE4_LOSS_IDENTITY_V4_V1: &str = "terminal_reinforce_value/v4-candidate";

/// The Copeland convention this build implements, recorded verbatim in the
/// routing record so a reader never has to infer it.
pub const CYCLE4_COPELAND_CONVENTION_V1: &str = "pairwise-wins-minus-pairwise-losses";

/// Largest file the CP7 guard will read while proving an outcome artifact
/// does not name an endpoint. A larger one cannot be cleared and is refused.
const CP7_GUARD_MAX_FILE_BYTES_V1: u64 = 64 * 1024 * 1024;
/// Bounds on the guard's own walk, so a pathological tree cannot turn the
/// selector into a filesystem crawler.
const CP7_GUARD_MAX_DEPTH_V1: usize = 32;
const CP7_GUARD_MAX_ENTRIES_V1: usize = 200_000;

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cycle4RoutingErrorV1 {
    code: &'static str,
    detail: String,
}

impl Cycle4RoutingErrorV1 {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl Display for Cycle4RoutingErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl Error for Cycle4RoutingErrorV1 {}

type Result<T> = std::result::Result<T, Cycle4RoutingErrorV1>;

// ---------------------------------------------------------------------
// M2 panel wire shape (what run_m2_common_root_panel_v1.py emits)
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2PoolSlotV1 {
    pub slot_index: u64,
    pub role: String,
    pub weight_units: u64,
    pub root_allocation: u64,
    pub store_generation: u64,
    pub source_run_sha256: String,
    pub checkpoint_manifest_sha256: String,
    pub checkpoint_payload_sha256: String,
    pub model_parameter_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2EndpointV1 {
    pub endpoint_id: String,
    pub store_generation: u64,
    pub run_sha256: String,
    pub identity_bundle_sha256: String,
    pub checkpoint_manifest_sha256: String,
    pub checkpoint_payload_sha256: String,
    pub model_parameter_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2MatchupV1 {
    pub endpoint_id: String,
    pub slot_index: u64,
    pub evaluation_seed: u64,
    pub pair_count: u64,
    pub game_count: u64,
    pub first_root_index: u64,
    pub outcome_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2LegsV1 {
    pub p0: i8,
    pub p1: i8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2RootV1 {
    pub root_index: u64,
    pub slot_index: u64,
    pub pair_index: u64,
    pub environment_seed: u64,
    pub legs: BTreeMap<String, Cycle4M2LegsV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2StatisticsV1 {
    pub root_count: u64,
    pub delta_pp: RealV1,
    pub standard_deviation_pp: RealV1,
    pub standard_error_pp: RealV1,
    pub ci_low_pp: RealV1,
    pub ci_high_pp: RealV1,
    pub one_sided_lower_bound_pp: RealV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2DiagnosticsV1 {
    pub legacy_integer_net: i64,
    pub legacy_integer_net_p0: i64,
    pub legacy_integer_net_p1: i64,
    pub gates_nothing: bool,
    pub confidence_sequence_computed: bool,
    pub confidence_sequence_reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2ComparisonV1 {
    pub endpoint_a: String,
    pub endpoint_b: String,
    pub pooled: Cycle4M2StatisticsV1,
    pub p0_stratum: Cycle4M2StatisticsV1,
    pub p1_stratum: Cycle4M2StatisticsV1,
    pub diagnostics: Cycle4M2DiagnosticsV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M2PanelV1 {
    pub schema: String,
    pub genesis_manifest_sha256: String,
    pub pool_arm: String,
    pub root_count: u64,
    pub base_seed: u64,
    pub opponent_seed_stride: u64,
    pub pool: Vec<Cycle4M2PoolSlotV1>,
    pub endpoints: Vec<Cycle4M2EndpointV1>,
    pub matchups: Vec<Cycle4M2MatchupV1>,
    pub roots: Vec<Cycle4M2RootV1>,
    pub comparisons: Vec<Cycle4M2ComparisonV1>,
}

fn all_endpoint_ids_v1() -> Vec<&'static str> {
    let mut ids = CYCLE4_ARM_ENDPOINT_IDS_V1.to_vec();
    ids.push(CYCLE4_BASELINE_ENDPOINT_ID_V1);
    ids
}

/// Decodes and structurally validates one M2 panel document.
pub fn decode_cycle4_m2_panel_v1(bytes: &[u8]) -> Result<Cycle4M2PanelV1> {
    let panel: Cycle4M2PanelV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid).map_err(
            |error| {
                Cycle4RoutingErrorV1::new("cycle4_routing_v1_canonical_json", error.to_string())
            },
        )?;
    if panel.schema != CYCLE4_M2_PANEL_SCHEMA_V1 {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_panel_schema",
            format!("unexpected schema {}", panel.schema),
        ));
    }
    if panel.root_count != CYCLE4_ROUTING_ROOT_COUNT_V1
        || panel.roots.len() as u64 != CYCLE4_ROUTING_ROOT_COUNT_V1
    {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_panel_root_count",
            format!(
                "the panel declares {} roots and carries {}; the amendment pins {}",
                panel.root_count,
                panel.roots.len(),
                CYCLE4_ROUTING_ROOT_COUNT_V1
            ),
        ));
    }
    let expected_ids = all_endpoint_ids_v1();
    let declared: Vec<&str> = panel
        .endpoints
        .iter()
        .map(|endpoint| endpoint.endpoint_id.as_str())
        .collect();
    if declared != expected_ids {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_panel_endpoints",
            format!("the panel must carry exactly {expected_ids:?} in that order"),
        ));
    }
    let mut seeds = std::collections::BTreeSet::new();
    for (ordinal, root) in panel.roots.iter().enumerate() {
        if root.root_index != ordinal as u64 {
            return Err(Cycle4RoutingErrorV1::new(
                "cycle4_routing_v1_panel_root_order",
                format!("root {ordinal} declares index {}", root.root_index),
            ));
        }
        if !seeds.insert(root.environment_seed) {
            return Err(Cycle4RoutingErrorV1::new(
                "cycle4_routing_v1_panel_seed_reuse",
                format!("environment seed {} appears twice", root.environment_seed),
            ));
        }
        for endpoint_id in &expected_ids {
            let legs = root.legs.get(*endpoint_id).ok_or_else(|| {
                Cycle4RoutingErrorV1::new(
                    "cycle4_routing_v1_panel_root_legs",
                    format!("root {ordinal} was not played by {endpoint_id}"),
                )
            })?;
            if !matches!(legs.p0, -1..=1) || !matches!(legs.p1, -1..=1) {
                return Err(Cycle4RoutingErrorV1::new(
                    "cycle4_routing_v1_panel_root_legs",
                    format!("root {ordinal} carries a terminal rank outside -1..1"),
                ));
            }
        }
        if root.legs.len() != expected_ids.len() {
            return Err(Cycle4RoutingErrorV1::new(
                "cycle4_routing_v1_panel_root_legs",
                format!("root {ordinal} names an endpoint the panel does not declare"),
            ));
        }
    }
    Ok(panel)
}

// ---------------------------------------------------------------------
// The estimator, recomputed
// ---------------------------------------------------------------------

/// `Y = I(win) + 0.5 * I(draw) = (rank + 1) / 2` for a terminal rank in
/// `{-1, 0, 1}`.
fn leg_score_v1(rank: i8) -> f64 {
    (f64::from(rank) + 1.0) / 2.0
}

/// Which per-root quantity a comparison is over.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StratumV1 {
    Pooled,
    P0,
    P1,
}

fn root_score_v1(legs: Cycle4M2LegsV1, stratum: StratumV1) -> f64 {
    match stratum {
        StratumV1::Pooled => (leg_score_v1(legs.p0) + leg_score_v1(legs.p1)) / 2.0,
        StratumV1::P0 => leg_score_v1(legs.p0),
        StratumV1::P1 => leg_score_v1(legs.p1),
    }
}

/// The fixed-N root-cluster paired statistics in percentage points, in
/// exactly the operation order `run_m2_common_root_panel_v1.py`'s
/// `paired_statistics` uses: a plain sequential sum for the mean, a two-pass
/// sample standard deviation, then `se = sd / sqrt(n)` and the z multiples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cycle4PairedStatisticsV1 {
    pub root_count: u64,
    pub delta_pp: f64,
    pub standard_deviation_pp: f64,
    pub standard_error_pp: f64,
    pub ci_low_pp: f64,
    pub ci_high_pp: f64,
    pub one_sided_lower_bound_pp: f64,
}

#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn paired_statistics_v1(differences: &[f64]) -> Cycle4PairedStatisticsV1 {
    let count = differences.len();
    let mut total = 0.0_f64;
    for value in differences {
        total += *value;
    }
    let mean = total / count as f64;
    let mut squares = 0.0_f64;
    for value in differences {
        let deviation = *value - mean;
        squares += deviation * deviation;
    }
    let variance = squares / (count - 1) as f64;
    let standard_deviation = variance.sqrt();
    let standard_error = standard_deviation / (count as f64).sqrt();
    let delta_pp = 100.0 * mean;
    let standard_error_pp = 100.0 * standard_error;
    Cycle4PairedStatisticsV1 {
        root_count: count as u64,
        delta_pp,
        standard_deviation_pp: 100.0 * standard_deviation,
        standard_error_pp,
        ci_low_pp: delta_pp - CYCLE4_Z_TWO_SIDED_95_V1 * standard_error_pp,
        ci_high_pp: delta_pp + CYCLE4_Z_TWO_SIDED_95_V1 * standard_error_pp,
        one_sided_lower_bound_pp: delta_pp - CYCLE4_Z_ONE_SIDED_95_V1 * standard_error_pp,
    }
}

fn recompute_stratum_v1(
    panel: &Cycle4M2PanelV1,
    endpoint_a: &str,
    endpoint_b: &str,
    stratum: StratumV1,
) -> Cycle4PairedStatisticsV1 {
    let differences: Vec<f64> = panel
        .roots
        .iter()
        .map(|root| {
            let a = root
                .legs
                .get(endpoint_a)
                .copied()
                .unwrap_or(Cycle4M2LegsV1 { p0: 0, p1: 0 });
            let b = root
                .legs
                .get(endpoint_b)
                .copied()
                .unwrap_or(Cycle4M2LegsV1 { p0: 0, p1: 0 });
            root_score_v1(a, stratum) - root_score_v1(b, stratum)
        })
        .collect();
    paired_statistics_v1(&differences)
}

fn cross_check_statistics_v1(
    label: &str,
    recomputed: Cycle4PairedStatisticsV1,
    declared: &Cycle4M2StatisticsV1,
) -> Result<()> {
    let mismatch = |field: &str, expected: f64, found: &RealV1| {
        Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_panel_statistic_mismatch",
            format!(
                "{label}.{field}: the panel declares {} but this build recomputes {} from the \
                 panel's own root table",
                found.f64_bits,
                f64_bits_hex_v1(expected)
            ),
        )
    };
    if declared.root_count != recomputed.root_count {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_panel_statistic_mismatch",
            format!("{label}.root_count: declared {}", declared.root_count),
        ));
    }
    for (field, expected, found) in [
        ("delta_pp", recomputed.delta_pp, &declared.delta_pp),
        (
            "standard_deviation_pp",
            recomputed.standard_deviation_pp,
            &declared.standard_deviation_pp,
        ),
        (
            "standard_error_pp",
            recomputed.standard_error_pp,
            &declared.standard_error_pp,
        ),
        ("ci_low_pp", recomputed.ci_low_pp, &declared.ci_low_pp),
        ("ci_high_pp", recomputed.ci_high_pp, &declared.ci_high_pp),
        (
            "one_sided_lower_bound_pp",
            recomputed.one_sided_lower_bound_pp,
            &declared.one_sided_lower_bound_pp,
        ),
    ] {
        if found.f64_bits != f64_bits_hex_v1(expected) {
            return Err(mismatch(field, expected, found));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------
// The decision
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4RoutingStatisticsWireV1 {
    pub root_count: u64,
    pub delta_pp: RealV1,
    pub standard_deviation_pp: RealV1,
    pub standard_error_pp: RealV1,
    pub ci_low_pp: RealV1,
    pub ci_high_pp: RealV1,
    pub one_sided_lower_bound_pp: RealV1,
}

impl Cycle4RoutingStatisticsWireV1 {
    fn from_v1(statistics: Cycle4PairedStatisticsV1) -> Self {
        Self {
            root_count: statistics.root_count,
            delta_pp: RealV1::from_f64_v1(statistics.delta_pp),
            standard_deviation_pp: RealV1::from_f64_v1(statistics.standard_deviation_pp),
            standard_error_pp: RealV1::from_f64_v1(statistics.standard_error_pp),
            ci_low_pp: RealV1::from_f64_v1(statistics.ci_low_pp),
            ci_high_pp: RealV1::from_f64_v1(statistics.ci_high_pp),
            one_sided_lower_bound_pp: RealV1::from_f64_v1(statistics.one_sided_lower_bound_pp),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4RoutingComparisonWireV1 {
    pub endpoint_a: String,
    pub endpoint_b: String,
    pub pooled: Cycle4RoutingStatisticsWireV1,
    pub p0_stratum: Cycle4RoutingStatisticsWireV1,
    pub p1_stratum: Cycle4RoutingStatisticsWireV1,
    pub ci_excludes_zero: bool,
    pub point_at_least_separability_threshold: bool,
    pub separable: bool,
    /// `endpoint_a`, `endpoint_b`, or the empty string when inseparable.
    pub winner: String,
    pub diagnostics: Cycle4M2DiagnosticsV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4RoutingArmWireV1 {
    pub endpoint_id: String,
    pub eligible: bool,
    pub eligibility_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub m3_report_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub m3_verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub m3_failures: Option<Vec<String>>,
    pub copeland_wins: u64,
    pub copeland_losses: u64,
    pub copeland_score: i64,
    pub rank: u64,
    pub tie_break_ordinal: u64,
    pub versus_g896: Cycle4RoutingVersusBaselineWireV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4RoutingVersusBaselineWireV1 {
    pub pooled_one_sided_lower_bound_pp: RealV1,
    pub p1_stratum_one_sided_lower_bound_pp: RealV1,
    pub pooled_clause_holds: bool,
    pub p1_clause_holds: bool,
    pub carries: bool,
    /// True for each arm actually reached while walking rank order; an arm
    /// after the carrier is never tested.
    pub tested: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4RoutingRecipeWireV1 {
    pub arm_kind: String,
    pub trainer_loss_identity: String,
    pub centered_baseline: bool,
    pub refresh_machinery: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4RoutingThresholdsWireV1 {
    pub root_count: u64,
    pub separability_min_abs_point_pp: RealV1,
    pub carry_pooled_lower_bound_pp: RealV1,
    pub carry_p1_stratum_lower_bound_pp: RealV1,
    pub z_two_sided_95: RealV1,
    pub z_one_sided_95: RealV1,
    pub copeland_convention: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4RoutingInputsWireV1 {
    pub m2_panel_sha256: String,
    pub m2_genesis_manifest_sha256: String,
    pub m2_pool_arm: String,
    pub m2_base_seed: u64,
    pub m3_static_rb_report_sha256: String,
    pub m3_treatment_rb_report_sha256: String,
    pub m3_reference_document_sha256: String,
    pub cp7_evidence_root_checked: bool,
    pub endpoints: Vec<Cycle4M2EndpointV1>,
    pub pool: Vec<Cycle4M2PoolSlotV1>,
}

/// The published routing record. Its own bytes are its identity; the bin
/// prints their SHA-256.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4RoutingRecordV1 {
    pub schema: String,
    /// `"CARRY"` or `"NO_CARRY"`.
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carried_endpoint_id: Option<String>,
    pub parent_run_sha256: String,
    pub parent_checkpoint_manifest_sha256: String,
    pub parent_store_generation: u64,
    pub recipe: Cycle4RoutingRecipeWireV1,
    pub thresholds: Cycle4RoutingThresholdsWireV1,
    pub inputs: Cycle4RoutingInputsWireV1,
    pub rank_order: Vec<String>,
    pub arms: Vec<Cycle4RoutingArmWireV1>,
    pub comparisons: Vec<Cycle4RoutingComparisonWireV1>,
}

pub const CYCLE4_ROUTING_OUTCOME_CARRY_V1: &str = "CARRY";
pub const CYCLE4_ROUTING_OUTCOME_NO_CARRY_V1: &str = "NO_CARRY";

/// Everything the selector reads, as bytes plus the two pinned cycle-3
/// fallback identities. Taking bytes rather than paths keeps the whole
/// decision pure and directly testable.
#[derive(Clone, Debug)]
pub struct Cycle4RoutingInputsV1 {
    pub m2_panel_bytes: Vec<u8>,
    pub m3_static_rb_bytes: Vec<u8>,
    pub m3_treatment_rb_bytes: Vec<u8>,
    /// The cycle-3 focal Store's own generation-2048 checkpoint manifest
    /// SHA-256 and run SHA-256: the NO CARRY parent. Recorded on every
    /// branch so the record always states what the fallback would have been.
    pub cycle3_g2048_run_sha256: String,
    pub cycle3_g2048_checkpoint_manifest_sha256: String,
}

fn recipe_for_v1(arm_kind: &str) -> Result<Cycle4RoutingRecipeWireV1> {
    match arm_kind {
        "control-r" => Ok(Cycle4RoutingRecipeWireV1 {
            arm_kind: arm_kind.to_owned(),
            trainer_loss_identity: CYCLE4_LOSS_IDENTITY_V3_V1.to_owned(),
            centered_baseline: false,
            refresh_machinery: true,
        }),
        "static-rb" => Ok(Cycle4RoutingRecipeWireV1 {
            arm_kind: arm_kind.to_owned(),
            trainer_loss_identity: CYCLE4_LOSS_IDENTITY_V4_V1.to_owned(),
            centered_baseline: true,
            refresh_machinery: false,
        }),
        "treatment-rb" => Ok(Cycle4RoutingRecipeWireV1 {
            arm_kind: arm_kind.to_owned(),
            trainer_loss_identity: CYCLE4_LOSS_IDENTITY_V4_V1.to_owned(),
            centered_baseline: true,
            refresh_machinery: true,
        }),
        other => Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_unknown_arm_kind",
            format!("unknown arm kind {other}"),
        )),
    }
}

fn hex64_or_reject_v1(value: &str, what: &str) -> Result<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Ok(());
    }
    Err(Cycle4RoutingErrorV1::new(
        "cycle4_routing_v1_invalid_digest",
        format!("{what} is not 64 lower-hex characters: {value}"),
    ))
}

fn m3_report_for_v1(endpoint_id: &str, bytes: &[u8]) -> Result<(Cycle4M3AuditReportV1, String)> {
    let report = decode_cycle4_m3_audit_report_v1(bytes).map_err(|error| {
        Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_m3_report",
            format!("{endpoint_id}: {error}"),
        )
    })?;
    if report.arm_kind != endpoint_id {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_m3_report_arm",
            format!(
                "the report supplied for {endpoint_id} declares arm_kind {}",
                report.arm_kind
            ),
        ));
    }
    Ok((report, lower_hex_raw32_v1(sha256_v1(bytes))))
}

/// Applies sections B and C and builds the routing record's canonical bytes.
///
/// Pure: no filesystem access, no clock, no environment. The CP7 guard is a
/// separate call the bin makes first ([`assert_cp7_evidence_absent_v1`]);
/// `cp7_evidence_root_checked` records that it ran.
pub fn decide_cycle4_routing_v1(inputs: &Cycle4RoutingInputsV1) -> Result<Vec<u8>> {
    hex64_or_reject_v1(&inputs.cycle3_g2048_run_sha256, "--cycle3-g2048-run-sha256")?;
    hex64_or_reject_v1(
        &inputs.cycle3_g2048_checkpoint_manifest_sha256,
        "--cycle3-g2048-checkpoint-manifest-sha256",
    )?;
    let panel = decode_cycle4_m2_panel_v1(&inputs.m2_panel_bytes)?;
    let panel_sha256 = lower_hex_raw32_v1(sha256_v1(&inputs.m2_panel_bytes));

    let (static_report, static_sha256) = m3_report_for_v1("static-rb", &inputs.m3_static_rb_bytes)?;
    let (treatment_report, treatment_sha256) =
        m3_report_for_v1("treatment-rb", &inputs.m3_treatment_rb_bytes)?;
    // Both v4 arms must be judged against ONE reference statistic, or their
    // dispersion verdicts are not comparable and the ranking is not the
    // amendment's.
    if static_report.inputs.reference_document_sha256
        != treatment_report.inputs.reference_document_sha256
    {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_m3_reference_drift",
            "the two v4 arms' M3 reports bind different reference documents",
        ));
    }

    // ---- Section B: the pairwise comparisons, recomputed and cross-checked.
    let mut declared_by_pair: BTreeMap<(String, String), &Cycle4M2ComparisonV1> = BTreeMap::new();
    for comparison in &panel.comparisons {
        declared_by_pair.insert(
            (comparison.endpoint_a.clone(), comparison.endpoint_b.clone()),
            comparison,
        );
    }

    let ordered_ids = all_endpoint_ids_v1();
    let mut comparisons = Vec::new();
    let mut wins: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut losses: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut versus_baseline: BTreeMap<
        &'static str,
        (Cycle4PairedStatisticsV1, Cycle4PairedStatisticsV1),
    > = BTreeMap::new();
    for endpoint_id in CYCLE4_ARM_ENDPOINT_IDS_V1 {
        wins.insert(endpoint_id, 0);
        losses.insert(endpoint_id, 0);
    }

    for (index, endpoint_a) in ordered_ids.iter().copied().enumerate() {
        for endpoint_b in ordered_ids.iter().copied().skip(index + 1) {
            let declared = declared_by_pair
                .get(&(endpoint_a.to_owned(), endpoint_b.to_owned()))
                .copied()
                .ok_or_else(|| {
                    Cycle4RoutingErrorV1::new(
                        "cycle4_routing_v1_panel_comparison_missing",
                        format!("the panel carries no {endpoint_a} vs {endpoint_b} comparison"),
                    )
                })?;
            let pooled = recompute_stratum_v1(&panel, endpoint_a, endpoint_b, StratumV1::Pooled);
            let p0 = recompute_stratum_v1(&panel, endpoint_a, endpoint_b, StratumV1::P0);
            let p1 = recompute_stratum_v1(&panel, endpoint_a, endpoint_b, StratumV1::P1);
            let label = format!("{endpoint_a}-vs-{endpoint_b}");
            cross_check_statistics_v1(&format!("{label}.pooled"), pooled, &declared.pooled)?;
            cross_check_statistics_v1(&format!("{label}.p0_stratum"), p0, &declared.p0_stratum)?;
            cross_check_statistics_v1(&format!("{label}.p1_stratum"), p1, &declared.p1_stratum)?;

            let ci_excludes_zero = pooled.ci_low_pp > 0.0 || pooled.ci_high_pp < 0.0;
            let point_clears = pooled.delta_pp.abs() >= CYCLE4_SEPARABILITY_MIN_ABS_POINT_PP_V1;
            let ranked_pair = endpoint_b != CYCLE4_BASELINE_ENDPOINT_ID_V1;
            let separable = ci_excludes_zero && point_clears;
            let winner = if separable {
                if pooled.delta_pp > 0.0 {
                    endpoint_a.to_owned()
                } else {
                    endpoint_b.to_owned()
                }
            } else {
                String::new()
            };
            if ranked_pair && separable {
                // g896 never participates in ranking, so only arm-vs-arm
                // pairs move a Copeland score.
                if pooled.delta_pp > 0.0 {
                    *wins.entry(endpoint_a).or_insert(0) += 1;
                    *losses.entry(endpoint_b).or_insert(0) += 1;
                } else {
                    *wins.entry(endpoint_b).or_insert(0) += 1;
                    *losses.entry(endpoint_a).or_insert(0) += 1;
                }
            }
            if endpoint_b == CYCLE4_BASELINE_ENDPOINT_ID_V1 {
                versus_baseline.insert(endpoint_a, (pooled, p1));
            }
            comparisons.push(Cycle4RoutingComparisonWireV1 {
                endpoint_a: endpoint_a.to_owned(),
                endpoint_b: endpoint_b.to_owned(),
                pooled: Cycle4RoutingStatisticsWireV1::from_v1(pooled),
                p0_stratum: Cycle4RoutingStatisticsWireV1::from_v1(p0),
                p1_stratum: Cycle4RoutingStatisticsWireV1::from_v1(p1),
                ci_excludes_zero,
                point_at_least_separability_threshold: point_clears,
                separable,
                winner,
                diagnostics: declared.diagnostics.clone(),
            });
        }
    }

    // ---- Eligibility (section A's verdict, consumed here).
    let mut eligible: BTreeMap<&'static str, bool> = BTreeMap::new();
    eligible.insert("control-r", true);
    eligible.insert(
        "static-rb",
        static_report.verdict == CYCLE4_M3_VERDICT_PASS_V1,
    );
    eligible.insert(
        "treatment-rb",
        treatment_report.verdict == CYCLE4_M3_VERDICT_PASS_V1,
    );

    // ---- Ranking: Copeland score, ties toward the fixed order.
    #[allow(clippy::cast_possible_wrap)]
    let mut ranking: Vec<(&'static str, i64, u64)> = CYCLE4_ARM_ENDPOINT_IDS_V1
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, endpoint_id)| {
            let score = i64::try_from(wins[endpoint_id]).unwrap_or(0)
                - i64::try_from(losses[endpoint_id]).unwrap_or(0);
            (endpoint_id, score, ordinal as u64)
        })
        .collect();
    ranking.sort_by_key(|(_, score, ordinal)| (-*score, *ordinal));

    // ---- Section C: carry, in rank order, among eligible arms.
    let mut carried: Option<&'static str> = None;
    let mut tested: BTreeMap<&'static str, bool> = BTreeMap::new();
    for (endpoint_id, _, _) in ranking.iter().copied() {
        if carried.is_some() {
            break;
        }
        if !eligible[endpoint_id] {
            continue;
        }
        let (pooled, p1) = versus_baseline.get(endpoint_id).copied().ok_or_else(|| {
            Cycle4RoutingErrorV1::new(
                "cycle4_routing_v1_panel_comparison_missing",
                format!("the panel carries no {endpoint_id} vs g896 comparison"),
            )
        })?;
        tested.insert(endpoint_id, true);
        let pooled_holds = pooled.one_sided_lower_bound_pp > CYCLE4_CARRY_POOLED_LOWER_BOUND_PP_V1;
        let p1_holds = p1.one_sided_lower_bound_pp > CYCLE4_CARRY_P1_LOWER_BOUND_PP_V1;
        if pooled_holds && p1_holds {
            carried = Some(endpoint_id);
        }
    }

    // ---- The record.
    let rank_index: BTreeMap<&'static str, u64> = ranking
        .iter()
        .copied()
        .enumerate()
        .map(|(rank, (endpoint_id, _, _))| (endpoint_id, rank as u64))
        .collect();
    let mut arms = Vec::new();
    for (ordinal, endpoint_id) in CYCLE4_ARM_ENDPOINT_IDS_V1.iter().copied().enumerate() {
        let (pooled, p1) = versus_baseline[endpoint_id];
        let (report_sha256, verdict, failures, source) = match endpoint_id {
            "static-rb" => (
                Some(static_sha256.clone()),
                Some(static_report.verdict.clone()),
                Some(static_report.failures.clone()),
                "m3-centering-gate",
            ),
            "treatment-rb" => (
                Some(treatment_sha256.clone()),
                Some(treatment_report.verdict.clone()),
                Some(treatment_report.failures.clone()),
                "m3-centering-gate",
            ),
            _ => (None, None, None, "v3-arm-always-eligible"),
        };
        let carries = carried == Some(endpoint_id);
        arms.push(Cycle4RoutingArmWireV1 {
            endpoint_id: endpoint_id.to_owned(),
            eligible: eligible[endpoint_id],
            eligibility_source: source.to_owned(),
            m3_report_sha256: report_sha256,
            m3_verdict: verdict,
            m3_failures: failures,
            copeland_wins: wins[endpoint_id],
            copeland_losses: losses[endpoint_id],
            copeland_score: i64::try_from(wins[endpoint_id]).unwrap_or(0)
                - i64::try_from(losses[endpoint_id]).unwrap_or(0),
            rank: rank_index[endpoint_id],
            tie_break_ordinal: ordinal as u64,
            versus_g896: Cycle4RoutingVersusBaselineWireV1 {
                pooled_one_sided_lower_bound_pp: RealV1::from_f64_v1(
                    pooled.one_sided_lower_bound_pp,
                ),
                p1_stratum_one_sided_lower_bound_pp: RealV1::from_f64_v1(
                    p1.one_sided_lower_bound_pp,
                ),
                pooled_clause_holds: pooled.one_sided_lower_bound_pp
                    > CYCLE4_CARRY_POOLED_LOWER_BOUND_PP_V1,
                p1_clause_holds: p1.one_sided_lower_bound_pp > CYCLE4_CARRY_P1_LOWER_BOUND_PP_V1,
                carries,
                tested: tested.get(endpoint_id).copied().unwrap_or(false),
            },
        });
    }

    let (outcome, carried_endpoint_id, recipe, parent_run, parent_checkpoint, parent_generation) =
        match carried {
            Some(endpoint_id) => {
                let endpoint = panel
                    .endpoints
                    .iter()
                    .find(|candidate| candidate.endpoint_id == endpoint_id)
                    .ok_or_else(|| {
                        Cycle4RoutingErrorV1::new(
                            "cycle4_routing_v1_panel_endpoints",
                            format!("the panel declares no identity for {endpoint_id}"),
                        )
                    })?;
                (
                    CYCLE4_ROUTING_OUTCOME_CARRY_V1,
                    Some(endpoint_id.to_owned()),
                    recipe_for_v1(endpoint_id)?,
                    endpoint.run_sha256.clone(),
                    endpoint.checkpoint_manifest_sha256.clone(),
                    endpoint.store_generation,
                )
            }
            None => (
                CYCLE4_ROUTING_OUTCOME_NO_CARRY_V1,
                None,
                recipe_for_v1("control-r")?,
                inputs.cycle3_g2048_run_sha256.clone(),
                inputs.cycle3_g2048_checkpoint_manifest_sha256.clone(),
                2_048,
            ),
        };

    let record = Cycle4RoutingRecordV1 {
        schema: CYCLE4_ROUTING_RECORD_SCHEMA_V1.to_owned(),
        outcome: outcome.to_owned(),
        carried_endpoint_id,
        parent_run_sha256: parent_run,
        parent_checkpoint_manifest_sha256: parent_checkpoint,
        parent_store_generation: parent_generation,
        recipe,
        thresholds: Cycle4RoutingThresholdsWireV1 {
            root_count: CYCLE4_ROUTING_ROOT_COUNT_V1,
            separability_min_abs_point_pp: RealV1::from_f64_v1(
                CYCLE4_SEPARABILITY_MIN_ABS_POINT_PP_V1,
            ),
            carry_pooled_lower_bound_pp: RealV1::from_f64_v1(CYCLE4_CARRY_POOLED_LOWER_BOUND_PP_V1),
            carry_p1_stratum_lower_bound_pp: RealV1::from_f64_v1(CYCLE4_CARRY_P1_LOWER_BOUND_PP_V1),
            z_two_sided_95: RealV1::from_f64_v1(CYCLE4_Z_TWO_SIDED_95_V1),
            z_one_sided_95: RealV1::from_f64_v1(CYCLE4_Z_ONE_SIDED_95_V1),
            copeland_convention: CYCLE4_COPELAND_CONVENTION_V1.to_owned(),
        },
        inputs: Cycle4RoutingInputsWireV1 {
            m2_panel_sha256: panel_sha256,
            m2_genesis_manifest_sha256: panel.genesis_manifest_sha256.clone(),
            m2_pool_arm: panel.pool_arm.clone(),
            m2_base_seed: panel.base_seed,
            m3_static_rb_report_sha256: static_sha256,
            m3_treatment_rb_report_sha256: treatment_sha256,
            m3_reference_document_sha256: static_report.inputs.reference_document_sha256.clone(),
            cp7_evidence_root_checked: true,
            endpoints: panel.endpoints.clone(),
            pool: panel.pool.clone(),
        },
        rank_order: ranking
            .iter()
            .copied()
            .map(|(endpoint_id, _, _)| endpoint_id.to_owned())
            .collect(),
        arms,
        comparisons,
    };

    to_canonical_json_bytes_v1(&record, CanonicalJsonNullPolicyV1::Forbid).map_err(|error| {
        Cycle4RoutingErrorV1::new("cycle4_routing_v1_canonical_json", error.to_string())
    })
}

/// Decodes a published routing record (for a reader, and for the round-trip
/// test).
pub fn decode_cycle4_routing_record_v1(bytes: &[u8]) -> Result<Cycle4RoutingRecordV1> {
    let record: Cycle4RoutingRecordV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid).map_err(
            |error| {
                Cycle4RoutingErrorV1::new("cycle4_routing_v1_canonical_json", error.to_string())
            },
        )?;
    if record.schema != CYCLE4_ROUTING_RECORD_SCHEMA_V1 {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_record_schema",
            format!("unexpected schema {}", record.schema),
        ));
    }
    Ok(record)
}

// ---------------------------------------------------------------------
// The CP7 guard
// ---------------------------------------------------------------------

/// Refuses if `root` already holds a CP7 outcome artifact naming any of the
/// endpoints.
///
/// INTENT. Section D pins the freeze order: the routing output "is written to
/// an immutable, content-hashed lane record BEFORE any M1 CP7 byte becomes
/// readable", so that "M1 may inform Jack's continue/escalate decision; it
/// cannot alter the recorded parent, recipe, constants, or any later
/// selector". A process cannot prove that nobody has read a CP7 result, but
/// it can refuse to run once the artifacts that would carry one exist where
/// they would land. That is what this is: a guard, not a proof.
///
/// WHAT IT LOOKS AT. Every regular file under `root`, to depth
/// `CP7_GUARD_MAX_DEPTH_V1`. A file is refused when
/// - its NAME contains any endpoint's `checkpoint_manifest_sha256` or
///   `model_parameter_sha256`, whatever its extension; or
/// - its name contains `outcome` (case-insensitive) and ends `.json`, and
///   its bytes contain any of those identities.
///
/// A candidate outcome file larger than `CP7_GUARD_MAX_FILE_BYTES_V1` is
/// refused rather than skipped: absence cannot be established for content
/// that was not read. A missing or non-directory `root` is refused too, so a
/// mistyped path cannot pass the guard by naming nothing.
///
/// WHAT IT CANNOT SEE. Anything outside `root` -- another evidence root, a
/// copy on another machine, a result someone already read and remembers. The
/// guard narrows the accident; the freeze order remains a discipline.
pub fn assert_cp7_evidence_absent_v1(root: &Path, identities: &[String]) -> Result<()> {
    let metadata = fs::metadata(root).map_err(|error| {
        Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_cp7_evidence_root",
            format!(
                "{}: {error}; --cp7-evidence-root must name an existing directory so a mistyped \
                 path cannot pass the guard",
                root.display()
            ),
        )
    })?;
    if !metadata.is_dir() {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_cp7_evidence_root",
            format!("{} is not a directory", root.display()),
        ));
    }
    if identities.is_empty() {
        return Err(Cycle4RoutingErrorV1::new(
            "cycle4_routing_v1_cp7_evidence_root",
            "the guard needs at least one endpoint identity to look for",
        ));
    }
    let mut stack = vec![(root.to_path_buf(), 0_usize)];
    let mut visited = 0_usize;
    while let Some((directory, depth)) = stack.pop() {
        if depth > CP7_GUARD_MAX_DEPTH_V1 {
            return Err(Cycle4RoutingErrorV1::new(
                "cycle4_routing_v1_cp7_guard_bounds",
                format!(
                    "{} is deeper than the guard's {CP7_GUARD_MAX_DEPTH_V1}-level bound",
                    directory.display()
                ),
            ));
        }
        let entries = fs::read_dir(&directory).map_err(|error| {
            Cycle4RoutingErrorV1::new(
                "cycle4_routing_v1_cp7_evidence_root",
                format!("{}: {error}", directory.display()),
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                Cycle4RoutingErrorV1::new(
                    "cycle4_routing_v1_cp7_evidence_root",
                    format!("{}: {error}", directory.display()),
                )
            })?;
            visited += 1;
            if visited > CP7_GUARD_MAX_ENTRIES_V1 {
                return Err(Cycle4RoutingErrorV1::new(
                    "cycle4_routing_v1_cp7_guard_bounds",
                    format!(
                        "{} holds more than the guard's {CP7_GUARD_MAX_ENTRIES_V1}-entry bound",
                        root.display()
                    ),
                ));
            }
            // No-follow: symlink_metadata, so a link is never traversed and
            // never read as an outcome file.
            let file_type = entry.file_type().map_err(|error| {
                Cycle4RoutingErrorV1::new(
                    "cycle4_routing_v1_cp7_evidence_root",
                    format!("{}: {error}", directory.display()),
                )
            })?;
            if file_type.is_dir() {
                stack.push((entry.path(), depth + 1));
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy().to_string();
            let lowered = name.to_ascii_lowercase();
            for identity in identities {
                if lowered.contains(&identity.to_ascii_lowercase()) {
                    return Err(cp7_present_v1(&entry.path(), identity, "its name"));
                }
            }
            if !(lowered.contains("outcome") && lowered.ends_with(".json")) {
                continue;
            }
            let length = entry
                .metadata()
                .map_err(|error| {
                    Cycle4RoutingErrorV1::new(
                        "cycle4_routing_v1_cp7_evidence_root",
                        format!("{}: {error}", entry.path().display()),
                    )
                })?
                .len();
            if length > CP7_GUARD_MAX_FILE_BYTES_V1 {
                return Err(Cycle4RoutingErrorV1::new(
                    "cycle4_routing_v1_cp7_guard_bounds",
                    format!(
                        "{} is {length} bytes, past the guard's {CP7_GUARD_MAX_FILE_BYTES_V1}-byte \
                         read bound; absence cannot be established for content that was not read",
                        entry.path().display()
                    ),
                ));
            }
            let bytes = fs::read(entry.path()).map_err(|error| {
                Cycle4RoutingErrorV1::new(
                    "cycle4_routing_v1_cp7_evidence_root",
                    format!("{}: {error}", entry.path().display()),
                )
            })?;
            let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
            for identity in identities {
                if text.contains(&identity.to_ascii_lowercase()) {
                    return Err(cp7_present_v1(&entry.path(), identity, "its content"));
                }
            }
        }
    }
    Ok(())
}

fn cp7_present_v1(path: &Path, identity: &str, where_found: &str) -> Cycle4RoutingErrorV1 {
    Cycle4RoutingErrorV1::new(
        "cycle4_routing_v1_cp7_evidence_present",
        format!(
            "{} names endpoint identity {identity} in {where_found}; section D requires the \
             routing record to be written BEFORE any M1 CP7 byte becomes readable, so routing \
             refuses to run against an evidence root that already holds one",
            path.display()
        ),
    )
}

/// Every identity string the CP7 guard looks for: each endpoint's checkpoint
/// manifest and model parameter digests.
#[must_use]
pub fn cp7_guard_identities_v1(panel: &Cycle4M2PanelV1) -> Vec<String> {
    let mut identities = Vec::with_capacity(panel.endpoints.len() * 2);
    for endpoint in &panel.endpoints {
        identities.push(endpoint.checkpoint_manifest_sha256.clone());
        identities.push(endpoint.model_parameter_sha256.clone());
    }
    identities.sort();
    identities.dedup();
    identities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_cycle4_m3_audit_v1::{
        build_cycle4_m3_audit_report_v1, build_cycle4_m3_reference_document_v1,
        decode_cycle4_m3_reference_document_v1, Cycle4M3CellV1,
    };

    fn hex_v1(tag: u8) -> String {
        format!("{tag:02x}").repeat(32)
    }

    // ---- M3 report fixtures -------------------------------------------

    fn reference_bytes_v1() -> Vec<u8> {
        build_cycle4_m3_reference_document_v1(
            &crate::native_cycle4_m3_audit_v1::test_support_window_v1(
                crate::native_cycle4_m3_audit_v1::Cycle4M3ResidualModeV1::Raw,
                vec![test_cell_v1(1, "p0", 10_000, -0.008, 1.0)],
                10_000,
            ),
            None,
        )
        .expect("reference")
    }

    fn test_cell_v1(tag: u8, role: &str, decisions: u64, mean: f64, sd: f64) -> Cycle4M3CellV1 {
        Cycle4M3CellV1 {
            opponent_checkpoint_manifest_sha256: hex_v1(tag),
            role: role.to_owned(),
            decision_count: decisions,
            episode_count: decisions / 30,
            mean_residual: RealV1::from_f64_v1(mean),
            sample_standard_deviation: RealV1::from_f64_v1(sd),
            qualifies: decisions
                >= crate::native_cycle4_m3_audit_v1::CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1,
        }
    }

    /// An M3 report for `arm_kind` whose verdict is PASS or FAIL as asked.
    fn m3_report_bytes_v1(arm_kind: &str, pass: bool) -> Vec<u8> {
        let reference = decode_cycle4_m3_reference_document_v1(&reference_bytes_v1())
            .expect("decode reference");
        let cells = if pass {
            vec![test_cell_v1(1, "p0", 10_000, 0.001, 1.05)]
        } else {
            vec![test_cell_v1(1, "p0", 10_000, 0.9, 1.05)]
        };
        let window = crate::native_cycle4_m3_audit_v1::test_support_window_v1(
            crate::native_cycle4_m3_audit_v1::Cycle4M3ResidualModeV1::Centered,
            cells,
            10_000,
        );
        let (bytes, produced) =
            build_cycle4_m3_audit_report_v1(arm_kind, &window, &reference, &hex_v1(0x0d))
                .expect("report");
        assert_eq!(produced, pass);
        bytes
    }

    // ---- M2 panel fixtures --------------------------------------------

    /// Builds a panel where every root's legs come from `plan`, a closure
    /// mapping `(endpoint, root_index)` to the two leg ranks. The declared
    /// comparison table is produced by this module's own recomputation, so
    /// the fixture is always self-consistent unless a test deliberately
    /// edits it.
    fn panel_bytes_v1(plan: &dyn Fn(&str, u64) -> (i8, i8)) -> Vec<u8> {
        let ids = all_endpoint_ids_v1();
        let mut roots = Vec::new();
        for root_index in 0..CYCLE4_ROUTING_ROOT_COUNT_V1 {
            let mut legs = BTreeMap::new();
            for endpoint_id in &ids {
                let (p0, p1) = plan(endpoint_id, root_index);
                legs.insert((*endpoint_id).to_owned(), Cycle4M2LegsV1 { p0, p1 });
            }
            roots.push(Cycle4M2RootV1 {
                root_index,
                slot_index: root_index / 128,
                pair_index: root_index % 128,
                environment_seed: 900_000_000 + root_index,
                legs,
            });
        }
        let mut panel = Cycle4M2PanelV1 {
            schema: CYCLE4_M2_PANEL_SCHEMA_V1.to_owned(),
            genesis_manifest_sha256: hex_v1(0x21),
            pool_arm: "control-r".to_owned(),
            root_count: CYCLE4_ROUTING_ROOT_COUNT_V1,
            base_seed: 5_100_000_000,
            opponent_seed_stride: 1_000_000,
            pool: (0..8)
                .map(|slot_index| Cycle4M2PoolSlotV1 {
                    slot_index,
                    role: format!("slot-{slot_index}"),
                    weight_units: 125_000,
                    root_allocation: 128,
                    store_generation: 384,
                    source_run_sha256: hex_v1(0x30 + slot_index as u8),
                    checkpoint_manifest_sha256: hex_v1(0x40 + slot_index as u8),
                    checkpoint_payload_sha256: hex_v1(0x50 + slot_index as u8),
                    model_parameter_sha256: hex_v1(0x60 + slot_index as u8),
                })
                .collect(),
            endpoints: ids
                .iter()
                .enumerate()
                .map(|(ordinal, endpoint_id)| Cycle4M2EndpointV1 {
                    endpoint_id: (*endpoint_id).to_owned(),
                    store_generation: if *endpoint_id == CYCLE4_BASELINE_ENDPOINT_ID_V1 {
                        896
                    } else {
                        2_048
                    },
                    run_sha256: hex_v1(0x70 + ordinal as u8),
                    identity_bundle_sha256: hex_v1(0x80 + ordinal as u8),
                    checkpoint_manifest_sha256: hex_v1(0x90 + ordinal as u8),
                    checkpoint_payload_sha256: hex_v1(0xa0 + ordinal as u8),
                    model_parameter_sha256: hex_v1(0xb0 + ordinal as u8),
                })
                .collect(),
            matchups: Vec::new(),
            roots,
            comparisons: Vec::new(),
        };
        let mut comparisons = Vec::new();
        for (index, endpoint_a) in ids.iter().enumerate() {
            for endpoint_b in ids.iter().skip(index + 1) {
                let statistics = |stratum| {
                    let value = recompute_stratum_v1(&panel, endpoint_a, endpoint_b, stratum);
                    Cycle4M2StatisticsV1 {
                        root_count: value.root_count,
                        delta_pp: RealV1::from_f64_v1(value.delta_pp),
                        standard_deviation_pp: RealV1::from_f64_v1(value.standard_deviation_pp),
                        standard_error_pp: RealV1::from_f64_v1(value.standard_error_pp),
                        ci_low_pp: RealV1::from_f64_v1(value.ci_low_pp),
                        ci_high_pp: RealV1::from_f64_v1(value.ci_high_pp),
                        one_sided_lower_bound_pp: RealV1::from_f64_v1(
                            value.one_sided_lower_bound_pp,
                        ),
                    }
                };
                comparisons.push(Cycle4M2ComparisonV1 {
                    endpoint_a: (*endpoint_a).to_owned(),
                    endpoint_b: (*endpoint_b).to_owned(),
                    pooled: statistics(StratumV1::Pooled),
                    p0_stratum: statistics(StratumV1::P0),
                    p1_stratum: statistics(StratumV1::P1),
                    diagnostics: Cycle4M2DiagnosticsV1 {
                        legacy_integer_net: 0,
                        legacy_integer_net_p0: 0,
                        legacy_integer_net_p1: 0,
                        gates_nothing: true,
                        confidence_sequence_computed: false,
                        confidence_sequence_reason: "diagnostic only".to_owned(),
                    },
                });
            }
        }
        panel.comparisons = comparisons;
        to_canonical_json_bytes_v1(&panel, CanonicalJsonNullPolicyV1::Forbid).expect("panel bytes")
    }

    /// A plan that gives each endpoint a fixed win fraction, deterministic in
    /// the root index, with both legs agreeing (so pooled and P1 move
    /// together and a test can aim a delta precisely).
    fn win_fraction_plan_v1(
        fractions: &'static [(&'static str, u64)],
    ) -> impl Fn(&str, u64) -> (i8, i8) {
        move |endpoint_id, root_index| {
            let wins_per_1024 = fractions
                .iter()
                .find(|(id, _)| *id == endpoint_id)
                .map_or(512, |(_, wins)| *wins);
            let rank = if root_index < wins_per_1024 { 1 } else { -1 };
            (rank, rank)
        }
    }

    fn routing_inputs_v1(
        panel: Vec<u8>,
        static_pass: bool,
        treatment_pass: bool,
    ) -> Cycle4RoutingInputsV1 {
        Cycle4RoutingInputsV1 {
            m2_panel_bytes: panel,
            m3_static_rb_bytes: m3_report_bytes_v1("static-rb", static_pass),
            m3_treatment_rb_bytes: m3_report_bytes_v1("treatment-rb", treatment_pass),
            cycle3_g2048_run_sha256: hex_v1(0xc1),
            cycle3_g2048_checkpoint_manifest_sha256: hex_v1(0xc2),
        }
    }

    fn decide_v1(inputs: &Cycle4RoutingInputsV1) -> Cycle4RoutingRecordV1 {
        let bytes = decide_cycle4_routing_v1(inputs).expect("routing decision");
        decode_cycle4_routing_record_v1(&bytes).expect("decode record")
    }

    // ---- Tests ---------------------------------------------------------

    /// Every endpoint identical: no pair separates, every Copeland score is
    /// 0, and the tie breaks toward CONTROL-R, which then carries because it
    /// is exactly tied with g896.
    #[test]
    fn inseparable_arms_tie_toward_control_r_and_carry_v1() {
        let plan = win_fraction_plan_v1(&[]);
        let record = decide_v1(&routing_inputs_v1(panel_bytes_v1(&plan), true, true));
        assert_eq!(record.outcome, CYCLE4_ROUTING_OUTCOME_CARRY_V1);
        assert_eq!(record.carried_endpoint_id.as_deref(), Some("control-r"));
        assert_eq!(
            record.recipe.trainer_loss_identity,
            CYCLE4_LOSS_IDENTITY_V3_V1
        );
        assert!(!record.recipe.centered_baseline);
        assert_eq!(
            record.rank_order,
            vec!["control-r", "static-rb", "treatment-rb"]
        );
        for comparison in &record.comparisons {
            assert!(!comparison.separable, "{comparison:?}");
            assert!(comparison.winner.is_empty());
        }
        assert_eq!(record.parent_store_generation, 2_048);
    }

    /// TREATMENT-RB clearly best: it separates from both other arms, takes
    /// the top Copeland score, and carries.
    #[test]
    fn the_separably_best_eligible_arm_carries_v1() {
        // 620/1024 vs 512/1024 is a +10.5pp pooled delta on every root, far
        // past the 1.0pp separability floor.
        let plan = win_fraction_plan_v1(&[("treatment-rb", 620), ("static-rb", 512)]);
        let record = decide_v1(&routing_inputs_v1(panel_bytes_v1(&plan), true, true));
        assert_eq!(record.rank_order[0], "treatment-rb");
        assert_eq!(record.carried_endpoint_id.as_deref(), Some("treatment-rb"));
        assert_eq!(
            record.recipe.trainer_loss_identity,
            CYCLE4_LOSS_IDENTITY_V4_V1
        );
        assert!(record.recipe.centered_baseline && record.recipe.refresh_machinery);
        let treatment = record
            .arms
            .iter()
            .find(|arm| arm.endpoint_id == "treatment-rb")
            .expect("arm row");
        assert_eq!(treatment.copeland_wins, 2);
        assert_eq!(treatment.copeland_losses, 0);
        assert_eq!(treatment.copeland_score, 2);
        assert_eq!(treatment.rank, 0);
    }

    /// The same panel, but TREATMENT-RB fails its M3 centering gate: it
    /// still ranks first, and the carry walks past it to the next ELIGIBLE
    /// arm in rank order.
    #[test]
    fn an_ineligible_top_ranked_arm_is_skipped_v1() {
        let plan = win_fraction_plan_v1(&[("treatment-rb", 620), ("static-rb", 512)]);
        let record = decide_v1(&routing_inputs_v1(panel_bytes_v1(&plan), true, false));
        assert_eq!(record.rank_order[0], "treatment-rb");
        let treatment = record
            .arms
            .iter()
            .find(|arm| arm.endpoint_id == "treatment-rb")
            .expect("arm row");
        assert!(!treatment.eligible);
        assert_eq!(treatment.m3_verdict.as_deref(), Some("FAIL"));
        assert!(!treatment.versus_g896.tested);
        assert_eq!(record.outcome, CYCLE4_ROUTING_OUTCOME_CARRY_V1);
        assert_ne!(record.carried_endpoint_id.as_deref(), Some("treatment-rb"));
        assert_eq!(record.carried_endpoint_id.as_deref(), Some("control-r"));
    }

    /// Every eligible arm is materially worse than g896, so no candidate
    /// clears the non-inferiority clauses and routing outputs NO CARRY with
    /// the cycle-3 g2048 parent under CONTROL-R.
    #[test]
    fn no_carry_falls_back_to_cycle3_g2048_under_control_r_v1() {
        // 400/1024 vs g896's 512/1024 is -10.9pp: both the pooled and the
        // P1 lower bounds sit far below their floors.
        let plan = win_fraction_plan_v1(&[
            ("control-r", 400),
            ("static-rb", 400),
            ("treatment-rb", 400),
        ]);
        let inputs = routing_inputs_v1(panel_bytes_v1(&plan), true, true);
        let record = decide_v1(&inputs);
        assert_eq!(record.outcome, CYCLE4_ROUTING_OUTCOME_NO_CARRY_V1);
        assert!(record.carried_endpoint_id.is_none());
        assert_eq!(record.recipe.arm_kind, "control-r");
        assert_eq!(record.parent_run_sha256, hex_v1(0xc1));
        assert_eq!(record.parent_checkpoint_manifest_sha256, hex_v1(0xc2));
        assert_eq!(record.parent_store_generation, 2_048);
        for arm in &record.arms {
            assert!(arm.versus_g896.tested, "every eligible arm must be tested");
            assert!(!arm.versus_g896.carries);
            assert!(!arm.versus_g896.pooled_clause_holds);
        }
    }

    /// The pooled clause alone is not enough: an arm that clears
    /// `> -1.0 pp` pooled but fails `> -2.0 pp` on the P1 stratum does not
    /// carry. Built by giving the arm g896's P0 record and a worse P1 one.
    #[test]
    fn the_p1_stratum_clause_can_block_a_carry_on_its_own_v1() {
        let plan = |endpoint_id: &str, root_index: u64| -> (i8, i8) {
            if endpoint_id == CYCLE4_BASELINE_ENDPOINT_ID_V1 {
                return (
                    if root_index < 512 { 1 } else { -1 },
                    if root_index < 512 { 1 } else { -1 },
                );
            }
            // P0 identical to g896; P1 loses 30 of 1,024 roots more, which
            // is -2.93pp on the P1 stratum (past -2.0) but only -1.46pp
            // pooled -- still short of the pooled floor, so this test pins
            // the P1 clause by asserting the arm-level flags, not the
            // outcome alone.
            (
                if root_index < 512 { 1 } else { -1 },
                if root_index < 497 { 1 } else { -1 },
            )
        };
        let record = decide_v1(&routing_inputs_v1(panel_bytes_v1(&plan), true, true));
        let control = record
            .arms
            .iter()
            .find(|arm| arm.endpoint_id == "control-r")
            .expect("arm row");
        assert!(!control.versus_g896.p1_clause_holds);
        assert!(!control.versus_g896.carries);
        assert_eq!(record.outcome, CYCLE4_ROUTING_OUTCOME_NO_CARRY_V1);
    }

    /// Separability needs BOTH clauses. A tiny but significant delta (the CI
    /// excludes 0, the point is under 1.0 pp) is inseparable.
    #[test]
    fn a_significant_but_small_delta_is_not_separable_v1() {
        // 517/1024 vs 512/1024 is +0.488pp with a CI that excludes zero
        // (every root's difference is the same sign or zero, so the standard
        // error is tiny), but the point is under the 1.0pp floor.
        let plan = win_fraction_plan_v1(&[("static-rb", 517)]);
        let panel = panel_bytes_v1(&plan);
        let record = decide_v1(&routing_inputs_v1(panel, true, true));
        let comparison = record
            .comparisons
            .iter()
            .find(|row| row.endpoint_a == "control-r" && row.endpoint_b == "static-rb")
            .expect("comparison");
        assert!(comparison.ci_excludes_zero, "{comparison:?}");
        assert!(!comparison.point_at_least_separability_threshold);
        assert!(!comparison.separable);
        assert_eq!(record.rank_order[0], "control-r");
    }

    /// A panel whose declared statistics do not follow from its own root
    /// table is refused: the selector decides on numbers it recomputed.
    #[test]
    fn a_panel_with_edited_statistics_is_refused_v1() {
        let plan = win_fraction_plan_v1(&[]);
        let bytes = panel_bytes_v1(&plan);
        let mut panel = decode_cycle4_m2_panel_v1(&bytes).expect("decode");
        panel.comparisons[0].pooled.delta_pp = RealV1::from_f64_v1(9.0);
        let edited =
            to_canonical_json_bytes_v1(&panel, CanonicalJsonNullPolicyV1::Forbid).expect("encode");
        let inputs = routing_inputs_v1(edited, true, true);
        assert_eq!(
            decide_cycle4_routing_v1(&inputs)
                .expect_err("edited statistics must be refused")
                .code(),
            "cycle4_routing_v1_panel_statistic_mismatch"
        );
    }

    /// The two v4 arms must be judged against ONE reference statistic.
    #[test]
    fn m3_reports_binding_different_references_are_refused_v1() {
        let plan = win_fraction_plan_v1(&[]);
        let reference = decode_cycle4_m3_reference_document_v1(&reference_bytes_v1())
            .expect("decode reference");
        let window = crate::native_cycle4_m3_audit_v1::test_support_window_v1(
            crate::native_cycle4_m3_audit_v1::Cycle4M3ResidualModeV1::Centered,
            vec![test_cell_v1(1, "p0", 10_000, 0.001, 1.05)],
            10_000,
        );
        let (other, _) =
            build_cycle4_m3_audit_report_v1("treatment-rb", &window, &reference, &hex_v1(0xee))
                .expect("report");
        let inputs = Cycle4RoutingInputsV1 {
            m3_treatment_rb_bytes: other,
            ..routing_inputs_v1(panel_bytes_v1(&plan), true, true)
        };
        assert_eq!(
            decide_cycle4_routing_v1(&inputs)
                .expect_err("reference drift must be refused")
                .code(),
            "cycle4_routing_v1_m3_reference_drift"
        );
    }

    /// A report supplied under the wrong arm's flag is refused.
    #[test]
    fn a_report_for_the_wrong_arm_is_refused_v1() {
        let plan = win_fraction_plan_v1(&[]);
        let inputs = Cycle4RoutingInputsV1 {
            m3_static_rb_bytes: m3_report_bytes_v1("treatment-rb", true),
            ..routing_inputs_v1(panel_bytes_v1(&plan), true, true)
        };
        assert_eq!(
            decide_cycle4_routing_v1(&inputs)
                .expect_err("wrong arm must be refused")
                .code(),
            "cycle4_routing_v1_m3_report_arm"
        );
    }

    // ---- CP7 guard -----------------------------------------------------

    struct TempDirV1 {
        path: std::path::PathBuf,
    }

    impl TempDirV1 {
        fn new_v1(label: &str) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "mtg-kernel-cycle4-routing-{}-{label}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDirV1 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn the_cp7_guard_passes_an_empty_root_and_ignores_unrelated_files_v1() {
        let root = TempDirV1::new_v1("cp7-clean");
        fs::create_dir_all(root.path.join("panel-a")).expect("subdir");
        fs::write(root.path.join("panel-a").join("notes.txt"), b"nothing here").expect("write");
        fs::write(
            root.path.join("panel-a").join("outcome.json"),
            b"{\"schema\":\"unrelated\",\"candidate\":\"00\"}",
        )
        .expect("write");
        assert!(assert_cp7_evidence_absent_v1(&root.path, &[hex_v1(0x90)]).is_ok());
    }

    #[test]
    fn the_cp7_guard_refuses_an_outcome_naming_an_endpoint_v1() {
        let root = TempDirV1::new_v1("cp7-content");
        let identity = hex_v1(0x91);
        fs::write(
            root.path.join("m1-outcome.json"),
            format!("{{\"candidate\":{{\"checkpoint_manifest_sha256\":\"{identity}\"}}}}")
                .as_bytes(),
        )
        .expect("write");
        assert_eq!(
            assert_cp7_evidence_absent_v1(&root.path, &[identity])
                .expect_err("must refuse")
                .code(),
            "cycle4_routing_v1_cp7_evidence_present"
        );
    }

    #[test]
    fn the_cp7_guard_refuses_a_file_named_for_an_endpoint_v1() {
        let root = TempDirV1::new_v1("cp7-name");
        let identity = hex_v1(0x92);
        fs::write(root.path.join(format!("{identity}.bin")), b"opaque").expect("write");
        assert_eq!(
            assert_cp7_evidence_absent_v1(&root.path, &[identity])
                .expect_err("must refuse")
                .code(),
            "cycle4_routing_v1_cp7_evidence_present"
        );
    }

    #[test]
    fn the_cp7_guard_refuses_a_missing_root_v1() {
        let root = TempDirV1::new_v1("cp7-missing");
        let missing = root.path.join("not-here");
        assert_eq!(
            assert_cp7_evidence_absent_v1(&missing, &[hex_v1(0x93)])
                .expect_err("must refuse")
                .code(),
            "cycle4_routing_v1_cp7_evidence_root"
        );
    }

    #[test]
    fn cp7_guard_identities_cover_both_digests_per_endpoint_v1() {
        let plan = win_fraction_plan_v1(&[]);
        let panel = decode_cycle4_m2_panel_v1(&panel_bytes_v1(&plan)).expect("decode");
        let identities = cp7_guard_identities_v1(&panel);
        assert_eq!(identities.len(), 8);
        assert!(identities.contains(&hex_v1(0x90)));
        assert!(identities.contains(&hex_v1(0xb3)));
    }

    /// CROSS-LANGUAGE INTEGRATION. Runs the M2 runner's own test fixture
    /// through a real Python interpreter, then decodes the bytes it wrote
    /// with the selector's canonical-JSON decoder and re-derives every
    /// statistic from the root table. This is the only check that Python's
    /// canonical form really is the form `canonical_json_v1` accepts (key
    /// order, separators, the trailing LF, no float anywhere) AND that the
    /// two estimators agree bit for bit on a full 1,024-root panel.
    ///
    /// Skipped, not failed, when no `python` is on PATH or the scripts
    /// directory is not where this crate expects it: the same
    /// `self.skipTest` convention `test_run_payoff_panel_v1.py` uses when it
    /// shells out to the PowerShell wrapper suite.
    #[test]
    fn a_python_produced_panel_decodes_and_cross_checks_v1() {
        let scripts = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("scripts")
            .join("experiments")
            .join("population_v2_cycle4_v1");
        if !scripts.join("run_m2_common_root_panel_v1.py").is_file() {
            eprintln!("skipping: {} not found", scripts.display());
            return;
        }
        let temp = TempDirV1::new_v1("python-panel");
        let output = temp.path.join("m2-common-root-panel.json");
        let program = format!(
            "import sys; sys.path.insert(0, {scripts:?}); \
             from test_run_m2_common_root_panel_v1 import emit_panel_bytes; \
             emit_panel_bytes(sys.argv[1])",
            scripts = scripts.to_string_lossy()
        );
        let mut ran = false;
        for interpreter in ["python", "python3", "py"] {
            match std::process::Command::new(interpreter)
                .arg("-c")
                .arg(&program)
                .arg(&output)
                .output()
            {
                Ok(result) if result.status.success() => {
                    ran = true;
                    break;
                }
                Ok(result) => {
                    panic!(
                        "{interpreter} failed to emit the panel: {}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                }
                Err(_) => continue,
            }
        }
        if !ran {
            eprintln!("skipping: no python interpreter on PATH");
            return;
        }

        let panel_bytes = fs::read(&output).expect("read the python panel");
        let panel = decode_cycle4_m2_panel_v1(&panel_bytes)
            .expect("the python runner's canonical bytes must decode");
        assert_eq!(panel.root_count, CYCLE4_ROUTING_ROOT_COUNT_V1);
        assert_eq!(panel.comparisons.len(), 6);

        // The full selector over it: every declared statistic is re-derived
        // from the root table and must match bit for bit, or this errors.
        let record = decide_v1(&Cycle4RoutingInputsV1 {
            m2_panel_bytes: panel_bytes,
            m3_static_rb_bytes: m3_report_bytes_v1("static-rb", true),
            m3_treatment_rb_bytes: m3_report_bytes_v1("treatment-rb", true),
            cycle3_g2048_run_sha256: hex_v1(0xc1),
            cycle3_g2048_checkpoint_manifest_sha256: hex_v1(0xc2),
        });
        // The fixture gives treatment-rb the largest share, so it separates
        // and ranks first; the numbers themselves are the point, not the
        // outcome, but a degenerate panel would prove nothing.
        assert_eq!(record.rank_order[0], "treatment-rb");
        assert_eq!(record.outcome, CYCLE4_ROUTING_OUTCOME_CARRY_V1);
        let versus = record
            .comparisons
            .iter()
            .find(|row| row.endpoint_a == "treatment-rb" && row.endpoint_b == "g896")
            .expect("treatment vs g896");
        assert_ne!(
            versus.pooled.delta_pp.f64_bits, versus.p1_stratum.delta_pp.f64_bits,
            "the fixture must exercise a P1 stratum that is not a copy of pooled"
        );
    }

    #[test]
    fn paired_statistics_match_the_closed_form_v1() {
        // Half the roots at +1, half at -1: mean 0, sample sd sqrt(n/(n-1)).
        let differences: Vec<f64> = (0..1_024)
            .map(|index| if index < 512 { 1.0 } else { -1.0 })
            .collect();
        let statistics = paired_statistics_v1(&differences);
        assert_eq!(statistics.root_count, 1_024);
        assert!(statistics.delta_pp.abs() < 1e-12);
        let expected_sd = (1_024.0_f64 / 1_023.0).sqrt();
        assert!((statistics.standard_deviation_pp - 100.0 * expected_sd).abs() < 1e-9);
        assert!(statistics.ci_low_pp < 0.0 && statistics.ci_high_pp > 0.0);
        assert!(statistics.one_sided_lower_bound_pp < 0.0);
    }

    /// CROSS-LANGUAGE PIN. These bit patterns were produced by
    /// `run_m2_common_root_panel_v1.paired_statistics` on the same 1,024
    /// differences, and its own test asserts the same literals. The routing
    /// selector requires bit equality between the panel's declared numbers
    /// and its own recomputation, so this is the check that the two
    /// implementations really are one estimator; if either drifts, both
    /// tests fail rather than a campaign failing at freeze time.
    #[test]
    fn the_estimator_matches_the_python_runner_bit_for_bit_v1() {
        let mut differences = vec![0.0_f64; 512];
        differences.extend(std::iter::repeat_n(1.0_f64, 108));
        differences.extend(std::iter::repeat_n(0.0_f64, 404));
        assert_eq!(differences.len(), 1_024);
        let statistics = paired_statistics_v1(&differences);
        assert_eq!(f64_bits_hex_v1(statistics.delta_pp), "4025180000000000");
        assert_eq!(
            f64_bits_hex_v1(statistics.standard_deviation_pp),
            "403ebb0c379a43ae"
        );
        assert_eq!(
            f64_bits_hex_v1(statistics.standard_error_pp),
            "3feebb0c379a43ae"
        );
        assert_eq!(f64_bits_hex_v1(statistics.ci_low_pp), "4021544deab0b3cf");
        assert_eq!(f64_bits_hex_v1(statistics.ci_high_pp), "4028dbb2154f4c31");
        assert_eq!(
            f64_bits_hex_v1(statistics.one_sided_lower_bound_pp),
            "4021ef3dba71d602"
        );
    }

    #[test]
    fn leg_scores_follow_the_half_point_draw_convention_v1() {
        assert!((leg_score_v1(1) - 1.0).abs() < f64::EPSILON);
        assert!((leg_score_v1(0) - 0.5).abs() < f64::EPSILON);
        assert!(leg_score_v1(-1).abs() < f64::EPSILON);
        let split = Cycle4M2LegsV1 { p0: 1, p1: -1 };
        assert!((root_score_v1(split, StratumV1::Pooled) - 0.5).abs() < f64::EPSILON);
        assert!((root_score_v1(split, StratumV1::P0) - 1.0).abs() < f64::EPSILON);
        assert!(root_score_v1(split, StratumV1::P1).abs() < f64::EPSILON);
    }
}
