//! Cycle-4 M3 centering audit: the eligibility gate of the ratified
//! section-6 mechanical amendment
//! (`LEAD_CYCLE4_SECTION6_MECHANICAL_AMENDMENT_V2.md`, section A).
//!
//! For one v4 arm Store plus its baseline chain directory, over the FINAL
//! 512 updates, this module computes per CELL -- `(opponent checkpoint
//! identity, learner role)`, exactly `BaselineCellKeyV4` -- the learner
//! decision count, the mean CENTERED residual, and the per-cell SAMPLE
//! standard deviation of the centered residual, then applies the
//! amendment's total function:
//!
//! - a cell QUALIFIES at `CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1` decisions;
//! - FAIL if no cell qualifies, or if the qualifying cells cover less than
//!   `CYCLE4_M3_COVERAGE_FLOOR_PERCENT_V1`% of the window's decisions;
//! - PASS iff `max |mean centered residual|` over qualifying cells is
//!   `<= CYCLE4_M3_CENTERING_MAX_ABS_MEAN_V1` AND the decision-weighted mean
//!   of per-cell standard deviations is
//!   `<= CYCLE4_M3_DISPERSION_RATIO_MAX_V1 x` the reference statistic.
//!
//! # How `c_t` is recovered
//!
//! Exactly as the v4 evidence validator recovers it, by calling that
//! validator. The baseline state is replayed forward from the chain's empty
//! genesis state; for every update in the window,
//! [`validate_update_baseline_v4`] is handed the update's own persisted
//! evidence (adapted into the documented per-episode cursor walk), the
//! update's sidecar record, and the running prior state, and it enforces the
//! whole contract-section-3 transaction: source-update binding, strict lag
//! (`declared c_t == prior_state.c_for_cell_v4`), per-cell residual sums and
//! counts recomputed bit-exactly from the evidence terms, the successor
//! state, and the v4 policy sum. The audit's per-term `c_t` is then read
//! from that same prior state, so a residual this module centers can never
//! use a baseline the batch did not actually use.
//!
//! Updates BEFORE the window are advanced sidecar-only (apply the sidecar's
//! own observations, enforce strict lag against the running state and the
//! declared successor), the same replay rule
//! `native_cycle4_arm_v1::Cycle4BaselineChainAccessV1::replay_sidecar_v1`
//! uses in-boundary. Their evidence is not part of the audited statistic, so
//! re-reading several hundred megabytes of it to reach the window would buy
//! nothing; the sidecar chain from the empty genesis state still pins the
//! window's opening `c_t` to a value no single record could fabricate alone.
//!
//! # Evidence reading and its fail-closed argument
//!
//! Store evidence is read with a mirror of the v1 wire shape rather than
//! through `native_training_store_update_group_v1`, whose wire structs are
//! module-private and whose frozen contract this module does not touch. The
//! leaf records the audit actually consumes -- `episodes[]` and
//! `physical_terms[]` -- are FULL `deny_unknown_fields` mirrors, so an
//! evidence file carrying a shape this audit does not understand is
//! rejected. Their containers are partial (unknown container keys are
//! ignored), and the integrity of what is read is then established the
//! strong way instead of the structural way: every value, terminal return,
//! learner seat and opponent identity the audit folds is re-folded by
//! [`validate_update_baseline_v4`] into per-cell residual-sum bits and
//! counts that must equal the sidecar's declared ones exactly. Tampering
//! with any of them moves those bits.
//!
//! # Arithmetic
//!
//! The per-term residual is the f32 subtraction the trainer commits,
//! widened once: `f64::from(target_f32 - value_f32)` for the RAW mode and
//! `f64::from((target_f32 - value_f32) - c_t_f32)` for the CENTERED mode,
//! matching `native_training_store_update_group_v4`'s own
//! `residual`/`advantage` pair bit for bit. Per-cell mean and sample
//! standard deviation come from Welford's online algorithm in batch order
//! (episode, then term within episode), which is deterministic for a fixed
//! store and avoids the cancellation a sum-of-squares form would invite.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonNullPathSegmentV1,
    CanonicalJsonNullPolicyV1,
};
use crate::native_policy_baseline_state_v4::{
    BaselineCellKeyV4, BaselineObservationV4, BaselineRoleV4, NativeBaselineStateV4,
};
use crate::native_training_store_digest_v1::NativeTrainingStoreAtomSha256V1;
use crate::native_training_store_digest_v1::{
    lower_hex_raw32_v1, parse_lower_hex_raw32_v1, sha256_v1,
};
use crate::native_training_store_update_group_v1::{
    EPISODE_SCHEMA_V1, UPDATE_EVIDENCE_SCHEMA_V1, UPDATE_EVIDENCE_SHA256_IDENTITY_V1,
};
use crate::native_training_store_update_group_v4::{
    decode_update_baseline_record_v4, validate_update_baseline_v4, UpdateBaselineEpisodeViewV4,
    UpdateBaselineTermViewV4,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

/// Schema of the audit report this module publishes.
pub const CYCLE4_M3_AUDIT_SCHEMA_V2: &str = "mtg-kernel-cycle4-m3-audit/v2";
/// Schema of the reference-statistic document the audit consumes.
pub const CYCLE4_M3_REFERENCE_SCHEMA_V2: &str = "mtg-kernel-cycle4-m3-reference/v2";
const CYCLE4_M3_AUDIT_SCHEMA_LEGACY_V1: &str = "mtg-kernel-cycle4-m3-audit/v1";
const CYCLE4_M3_REFERENCE_SCHEMA_LEGACY_V1: &str = "mtg-kernel-cycle4-m3-reference/v1";

/// Amendment section A: "the FINAL 512 updates".
pub const CYCLE4_M3_WINDOW_UPDATES_V1: u64 = 512;
/// The cycle-3 focal reference window is pinned to updates 1537 through 2048.
pub const CYCLE4_M3_REFERENCE_WINDOW_FIRST_UPDATE_INDEX_V1: u64 = 1_537;
pub const CYCLE4_M3_REFERENCE_WINDOW_LAST_UPDATE_INDEX_V1: u64 = 2_048;
/// Amendment section A: "A cell QUALIFIES if it holds at least 1,000 learner
/// decisions in the window."
pub const CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1: u64 = 1_000;
/// Amendment section A: "FAIL ... if the qualifying cells together cover
/// less than 80% of the window's learner decisions". Kept as an integer
/// percentage so the coverage comparison is exact integer arithmetic and
/// never a float near-miss.
pub const CYCLE4_M3_COVERAGE_FLOOR_PERCENT_V1: u64 = 80;
/// Amendment section A: "max over qualifying cells of |mean centered
/// residual| must be <= 0.015 on the +/-1 scale."
pub const CYCLE4_M3_CENTERING_MAX_ABS_MEAN_V1: f64 = 0.015;
/// Amendment section A: "<= 1.10 times the same statistic computed on the
/// cycle-3 focal store's final 512 updates from the RAW residual."
pub const CYCLE4_M3_DISPERSION_RATIO_MAX_V1: f64 = 1.10;

const SEGMENT_DIRECTORY_V1: &str = "segments";
const SEGMENT_NAME_PREFIX_V1: &str = "segment-";
const CONTINUATION_INFIX_V1: &str = ".continuation-";
const CONTINUATION_SUFFIX_V1: &str = ".json";
const CHECKPOINT_DIRECTORY_V1: &str = "checkpoints";
const CHECKPOINT_NAME_PREFIX_V1: &str = "update-";
const CHECKPOINT_NAME_SUFFIX_V1: &str = ".checkpoint.json";
const FIXED_INDEX_DIGITS_V1: usize = 8;
/// Domain separator for the aggregate sidecar/evidence chain digests below.
const CHAIN_DIGEST_DOMAIN_V1: &[u8] = b"mtg-kernel-cycle4-m3-chain-digest/v1";

// The Store's own nullable paths, restated here because the continuation
// module's copies are private to it. A continuation file is decoded through
// `from_canonical_json_bytes_v1` under exactly this policy, which enforces
// the byte-exact canonical form as a side effect: a file that is not
// canonical, or that carries a null anywhere but these three places, is
// refused before a single residual is read.
const M3_PREVIOUS_CONTINUATION_NULL_PATH_V1: &[CanonicalJsonNullPathSegmentV1] =
    &[CanonicalJsonNullPathSegmentV1::ObjectKey(
        "previous_continuation_sha256",
    )];
const M3_PREVIOUS_UPDATE_NULL_PATH_V1: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("update_groups"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("previous_update_evidence_sha256"),
];
const M3_EPISODE_WINNER_NULL_PATH_V1: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("update_groups"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("evidence"),
    CanonicalJsonNullPathSegmentV1::ObjectKey("episodes"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("winner"),
];
const M3_CONTINUATION_NULL_PATHS_V1: &[&[CanonicalJsonNullPathSegmentV1]] = &[
    M3_PREVIOUS_CONTINUATION_NULL_PATH_V1,
    M3_PREVIOUS_UPDATE_NULL_PATH_V1,
    M3_EPISODE_WINNER_NULL_PATH_V1,
];
pub(crate) const M3_CONTINUATION_NULL_POLICY_V1: CanonicalJsonNullPolicyV1 =
    CanonicalJsonNullPolicyV1::AllowOnly(M3_CONTINUATION_NULL_PATHS_V1);

/// The same rule rooted at ONE update's `evidence` object, which is what the
/// `update_evidence_sha256` domain hashes.
const M3_EVIDENCE_WINNER_NULL_PATH_V1: &[CanonicalJsonNullPathSegmentV1] = &[
    CanonicalJsonNullPathSegmentV1::ObjectKey("episodes"),
    CanonicalJsonNullPathSegmentV1::AnyArrayElement,
    CanonicalJsonNullPathSegmentV1::ObjectKey("winner"),
];
const M3_EVIDENCE_NULL_PATHS_V1: &[&[CanonicalJsonNullPathSegmentV1]] =
    &[M3_EVIDENCE_WINNER_NULL_PATH_V1];
pub(crate) const M3_EVIDENCE_NULL_POLICY_V1: CanonicalJsonNullPolicyV1 =
    CanonicalJsonNullPolicyV1::AllowOnly(M3_EVIDENCE_NULL_PATHS_V1);

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

/// Every rejection carries a stable machine-readable code plus a detail
/// string that never contains anything but paths and indices the caller
/// supplied or the Store itself declared.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cycle4M3AuditErrorV1 {
    code: &'static str,
    detail: String,
}

impl Cycle4M3AuditErrorV1 {
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

impl Display for Cycle4M3AuditErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl Error for Cycle4M3AuditErrorV1 {}

type Result<T> = std::result::Result<T, Cycle4M3AuditErrorV1>;

// ---------------------------------------------------------------------
// Shared real-number encoding (canonical JSON forbids floating point)
// ---------------------------------------------------------------------

/// The IEEE-754 bit pattern of `value` as 16 lower-hex characters. The
/// canonical-JSON codec rejects floats outright
/// (`CanonicalJsonErrorKindV1::FloatingPointForbidden`), so every real
/// number in these documents crosses as its bits, exactly as
/// `native_training_store_update_group_v4`'s `residual_sum_f64_bits` does.
#[must_use]
pub fn f64_bits_hex_v1(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

/// Inverse of [`f64_bits_hex_v1`], rejecting anything but 16 lower-hex
/// characters decoding to a finite double.
pub fn parse_f64_bits_hex_v1(value: &str) -> Result<f64> {
    if value.len() != 16
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_invalid_f64_bits",
            format!("not 16 lower-hex characters: {value}"),
        ));
    }
    let bits = u64::from_str_radix(value, 16).map_err(|_| {
        Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_invalid_f64_bits",
            format!("unparseable: {value}"),
        )
    })?;
    let decoded = f64::from_bits(bits);
    if !decoded.is_finite() {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_invalid_f64_bits",
            format!("non-finite: {value}"),
        ));
    }
    Ok(decoded)
}

/// The shortest round-tripping decimal rendering of a double. DERIVED and
/// never authoritative: every consumer reads the `_f64_bits` sibling; this
/// exists so a human reading a routing record does not have to decode hex.
#[must_use]
pub fn f64_text_v1(value: f64) -> String {
    format!("{value:?}")
}

/// One real number as the pair every document in this family emits.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RealV1 {
    pub f64_bits: String,
    pub text: String,
}

impl RealV1 {
    #[must_use]
    pub fn from_f64_v1(value: f64) -> Self {
        Self {
            f64_bits: f64_bits_hex_v1(value),
            text: f64_text_v1(value),
        }
    }

    /// Decodes the AUTHORITATIVE half. `text` is deliberately not
    /// cross-checked here: it is a display rendering, one of these documents
    /// is written by Python (the M2 panel) whose shortest-round-trip
    /// formatter spells exponents differently from Rust's, and nothing in
    /// this family ever decides on it. Every consumer reads `f64_bits`.
    pub fn to_f64_v1(&self) -> Result<f64> {
        parse_f64_bits_hex_v1(&self.f64_bits)
    }
}

// ---------------------------------------------------------------------
// Residual mode
// ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cycle4M3ResidualModeV1 {
    /// `(target - value) - c_t` as committed: the amendment's eligibility
    /// statistic for a v4 arm.
    Centered,
    /// `target - value`: the reference statistic, computed the same way from
    /// the cycle-3 focal store, which has no baseline at all.
    Raw,
}

impl Cycle4M3ResidualModeV1 {
    #[must_use]
    pub const fn wire_v1(self) -> &'static str {
        match self {
            Self::Centered => "centered",
            Self::Raw => "raw",
        }
    }

    fn from_wire_v1(value: &str) -> Result<Self> {
        match value {
            "centered" => Ok(Self::Centered),
            "raw" => Ok(Self::Raw),
            other => Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_invalid_residual_mode",
                format!("unknown residual mode {other}"),
            )),
        }
    }
}

// ---------------------------------------------------------------------
// Evidence mirror
// ---------------------------------------------------------------------

/// FULL `deny_unknown_fields` mirror of `physical_terms[]`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicalTermReadV1 {
    joint_log_probability_f32_bits: String,
    value_f32_bits: String,
    terminal_return_i8: i8,
    #[allow(dead_code)]
    substep_count: u32,
}

/// FULL `deny_unknown_fields` mirror of `episodes[]`. Every field of the
/// frozen wire shape is named so an evidence record carrying anything this
/// audit does not understand is rejected instead of silently reinterpreted.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EpisodeReadV1 {
    schema: String,
    #[allow(dead_code)]
    episode_index: u64,
    #[allow(dead_code)]
    environment_seed_u64_hex: String,
    #[allow(dead_code)]
    deck_ids: [String; 2],
    #[allow(dead_code)]
    deck_hashes_u64_hex: [String; 2],
    learner_seat: String,
    learner_return: i8,
    #[allow(dead_code)]
    terminal_outcome: String,
    #[allow(dead_code)]
    winner: Option<String>,
    #[allow(dead_code)]
    terminal_classification: String,
    #[allow(dead_code)]
    terminal_code: String,
    #[allow(dead_code)]
    policy_step_count: u64,
    #[allow(dead_code)]
    physical_decision_count: u64,
    #[allow(dead_code)]
    learner_policy_step_count: u64,
    #[allow(dead_code)]
    opponent_policy_step_count: u64,
    learner_physical_decision_count: u64,
    #[allow(dead_code)]
    opponent_physical_decision_count: u64,
    #[allow(dead_code)]
    trajectory_sha256: String,
    #[serde(default)]
    #[allow(dead_code)]
    opponent_population_slot: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)]
    opponent_occupant_class: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    opponent_run_sha256: Option<String>,
    #[serde(default)]
    opponent_checkpoint_manifest_sha256: Option<String>,
    #[serde(default)]
    opponent_search_tier: Option<String>,
    #[serde(default)]
    opponent_search_authority_sha256: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    scoring_weight_version: Option<u64>,
    #[serde(default)]
    #[allow(dead_code)]
    consuming_update_version: Option<u64>,
}

/// PARTIAL mirror of one update's evidence: the container keys the audit
/// never reads (gauge, rollout counts, loss, progress, the model and
/// train-state digests) are ignored. See the module docs for why the
/// integrity argument does not rest on this struct.
#[derive(Clone, Debug, Deserialize)]
struct EvidenceReadV1 {
    schema: String,
    run_sha256: String,
    update_index: u64,
    learner_physical_decision_count: u64,
    physical_terms: Vec<PhysicalTermReadV1>,
    episodes: Vec<EpisodeReadV1>,
}

/// PARTIAL mirror of one `update_groups[]` entry.
#[derive(Clone, Debug, Deserialize)]
struct UpdateGroupReadV1 {
    update_index: u64,
    update_evidence_sha256: String,
    #[serde(default)]
    previous_update_evidence_sha256: Option<String>,
    evidence: EvidenceReadV1,
}

/// PARTIAL mirror of one segment continuation file.
#[derive(Clone, Debug, Deserialize)]
struct ContinuationReadV1 {
    update_groups: Vec<UpdateGroupReadV1>,
}

/// Index-only mirror for the first pass, which walks the whole Store to find
/// the tip, prove the update stream is contiguous, and verify the declared
/// evidence-digest chain end to end.
#[derive(Clone, Debug, Deserialize)]
struct ContinuationIndexReadV1 {
    update_groups: Vec<UpdateGroupIndexReadV1>,
}

#[derive(Clone, Debug, Deserialize)]
struct UpdateGroupIndexReadV1 {
    update_index: u64,
    update_evidence_sha256: String,
    #[serde(default)]
    previous_update_evidence_sha256: Option<String>,
}

/// Recomputes one update's `update_evidence_sha256` exactly as
/// `native_training_store_update_group_v1::update_evidence_sha256_v1` does:
/// the same domain separator, the same atom order, and the evidence object's
/// own canonical JSON bytes.
///
/// This is what makes the audit's DISPERSION statistic tamper-evident. The
/// v4 sidecar cross-check pins each cell's residual SUM and counts, so an
/// edit that moves two equal-policy-weight values in one cell in opposite
/// directions leaves every sidecar quantity untouched while changing that
/// cell's sample standard deviation. The digest moves; the sums do not.
fn recompute_update_evidence_sha256_v1(
    run_sha256: [u8; 32],
    update_index: u64,
    previous_update_evidence_sha256: Option<[u8; 32]>,
    evidence_cj: &[u8],
) -> Result<[u8; 32]> {
    let digest_error = |_| {
        Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_evidence_digest_recompute",
            format!("update {update_index}: atom hashing rejected an input"),
        )
    };
    let mut digest = NativeTrainingStoreAtomSha256V1::new();
    digest
        .atom("domain", UPDATE_EVIDENCE_SHA256_IDENTITY_V1.as_bytes())
        .map_err(digest_error)?;
    digest
        .atom("run_sha256", &run_sha256)
        .map_err(digest_error)?;
    digest
        .atom("update_index_u64be", &update_index.to_be_bytes())
        .map_err(digest_error)?;
    digest
        .atom(
            "previous_update_evidence_sha256",
            previous_update_evidence_sha256
                .as_ref()
                .map_or(&[][..], |value| value.as_slice()),
        )
        .map_err(digest_error)?;
    digest
        .atom("evidence_canonical_json", evidence_cj)
        .map_err(digest_error)?;
    Ok(digest.finalize())
}

fn io_error_v1(path: &Path, error: &std::io::Error) -> Cycle4M3AuditErrorV1 {
    Cycle4M3AuditErrorV1::new(
        "cycle4_m3_audit_v1_io",
        format!("{}: {error}", path.display()),
    )
}

fn decode_error_v1(path: &Path, detail: impl Display) -> Cycle4M3AuditErrorV1 {
    Cycle4M3AuditErrorV1::new(
        "cycle4_m3_audit_v1_evidence_decode",
        format!("{}: {detail}", path.display()),
    )
}

fn parse_fixed_index_v1(text: &str) -> Option<u64> {
    if text.len() != FIXED_INDEX_DIGITS_V1 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok()
}

/// Every `segments/segment-<8 digits>.continuation-<8 digits>.json` in the
/// Store, ascending by `(generation, continuation)`. Any other regular file
/// under `segments/` whose name starts with `segment-` and ends with
/// `.json` but is not a valid segment manifest or continuation name fails
/// closed: a Store with staging debris in it is not a Store this audit will
/// read.
fn list_continuation_paths_v1(store_root: &Path) -> Result<Vec<(u64, u64, PathBuf)>> {
    let directory = store_root.join(SEGMENT_DIRECTORY_V1);
    let entries = fs::read_dir(&directory).map_err(|error| io_error_v1(&directory, &error))?;
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| io_error_v1(&directory, &error))?;
        if !entry
            .file_type()
            .map_err(|error| io_error_v1(&directory, &error))?
            .is_file()
        {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(rest) = name.strip_prefix(SEGMENT_NAME_PREFIX_V1) else {
            continue;
        };
        let Some(rest) = rest.strip_suffix(CONTINUATION_SUFFIX_V1) else {
            continue;
        };
        // `segment-<8>.json` is the segment manifest, not a continuation.
        if parse_fixed_index_v1(rest).is_some() {
            continue;
        }
        let Some((generation_text, continuation_text)) = rest.split_once(CONTINUATION_INFIX_V1)
        else {
            return Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_segment_leaf",
                format!("unrecognized leaf in {}: {name}", directory.display()),
            ));
        };
        let (Some(generation), Some(continuation)) = (
            parse_fixed_index_v1(generation_text),
            parse_fixed_index_v1(continuation_text),
        ) else {
            return Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_segment_leaf",
                format!("unrecognized leaf in {}: {name}", directory.display()),
            ));
        };
        found.push((generation, continuation, entry.path()));
    }
    if found.is_empty() {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_empty_store",
            format!("{} holds no segment continuations", directory.display()),
        ));
    }
    found.sort_by_key(|(generation, continuation, _)| (*generation, *continuation));
    Ok(found)
}

// ---------------------------------------------------------------------
// Per-cell accumulation
// ---------------------------------------------------------------------

/// Welford's online mean and sum of squared deviations, in batch order.
#[derive(Clone, Copy, Debug, Default)]
struct CellAccumulatorV1 {
    count: u64,
    mean: f64,
    sum_squared_deviations: f64,
}

impl CellAccumulatorV1 {
    #[allow(clippy::cast_precision_loss)]
    fn observe_v1(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta_after = value - self.mean;
        self.sum_squared_deviations += delta * delta_after;
    }

    /// Sample standard deviation (denominator `count - 1`). A one-decision
    /// cell has no sample standard deviation; it can never qualify (the
    /// threshold is 1,000 decisions) but the statistic still has to be a
    /// number, and zero is the only honest one for a single point.
    #[allow(clippy::cast_precision_loss)]
    fn sample_standard_deviation_v1(&self) -> f64 {
        if self.count < 2 {
            return 0.0;
        }
        (self.sum_squared_deviations / (self.count - 1) as f64).sqrt()
    }
}

/// Domain prefix of the search-occupant cell identity (clarification V2.2 of
/// the section-6 amendment, ratified 2026-09-05).
pub const CYCLE4_M3_SEARCH_OCCUPANT_IDENTITY_PREFIX_V1: &str =
    "cycle4-m3-search-occupant-identity-v1";

/// The M3 cell identity of a search occupant: SHA-256 (64 lower hex) over the
/// UTF-8 string `cycle4-m3-search-occupant-identity-v1:<authority>:<tier>`.
/// A search occupant's episode record carries `opponent_search_authority_sha256`
/// and `opponent_search_tier` instead of a checkpoint manifest (cycle-3's pool
/// slot 6 is the case in the record); hashing both to 64 hex keeps every
/// consumer's identity handling unchanged while never colliding with a
/// checkpoint manifest hash.
pub fn search_occupant_cell_identity_v1(authority_sha256: &str, tier: &str) -> String {
    let payload =
        format!("{CYCLE4_M3_SEARCH_OCCUPANT_IDENTITY_PREFIX_V1}:{authority_sha256}:{tier}");
    format!("{:x}", Sha256::digest(payload.as_bytes()))
}

/// The search tiers the Store contract registers for a search occupant.
pub const CYCLE4_M3_SEARCH_OCCUPANT_TIERS_V1: [&str; 4] = ["t512", "t2048", "t8192", "t32768"];

fn is_sha256_lower_hex_v1(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// How an episode names its opponent for the M3 cell.
enum EpisodeCellIdentityV1 {
    /// A checkpoint manifest SHA-256, or a well-formed search-occupant identity.
    Identity(String),
    /// Neither a checkpoint manifest nor both search fields: no cell.
    Absent,
    /// Search fields present but malformed (authority not 64 lower hex, or a
    /// tier outside the registered set): refused, never hashed into a key.
    MalformedSearchOccupant(String),
}

/// The cell identity an episode contributes to: its opponent's checkpoint
/// manifest SHA-256 when it carries one; otherwise the search-occupant
/// identity when it carries a 64-lower-hex authority AND a registered tier
/// (both validated BEFORE hashing, so a malformed authority or an unknown
/// tier can never become a plausible key, and the `:`-joined payload is
/// injective because a validated authority contains no `:`); otherwise
/// absent, and the audit fails closed rather than inventing a cell.
fn episode_cell_identity_v1(episode: &EpisodeReadV1) -> EpisodeCellIdentityV1 {
    if let Some(manifest) = &episode.opponent_checkpoint_manifest_sha256 {
        if !manifest.is_empty() {
            return EpisodeCellIdentityV1::Identity(manifest.clone());
        }
    }
    match (
        &episode.opponent_search_authority_sha256,
        &episode.opponent_search_tier,
    ) {
        (Some(authority), Some(tier)) => {
            if !is_sha256_lower_hex_v1(authority) {
                return EpisodeCellIdentityV1::MalformedSearchOccupant(format!(
                    "opponent_search_authority_sha256 {authority:?} is not 64 lower hex"
                ));
            }
            if !CYCLE4_M3_SEARCH_OCCUPANT_TIERS_V1.contains(&tier.as_str()) {
                return EpisodeCellIdentityV1::MalformedSearchOccupant(format!(
                    "opponent_search_tier {tier:?} is not a registered tier"
                ));
            }
            EpisodeCellIdentityV1::Identity(search_occupant_cell_identity_v1(authority, tier))
        }
        _ => EpisodeCellIdentityV1::Absent,
    }
}

/// One cell's audited statistics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3CellV1 {
    pub opponent_checkpoint_manifest_sha256: String,
    pub role: String,
    pub decision_count: u64,
    pub episode_count: u64,
    pub mean_residual: RealV1,
    pub sample_standard_deviation: RealV1,
    pub qualifies: bool,
}

/// The audited window: what was read, from where, and the per-cell table.
#[derive(Clone, Debug)]
pub struct Cycle4M3WindowV1 {
    residual_mode: Cycle4M3ResidualModeV1,
    run_sha256: String,
    first_update_index: u64,
    last_update_index: u64,
    tip_update_evidence_sha256: String,
    /// SHA-256 of the Store's own checkpoint manifest at the window's last
    /// update, which is the identity the M2 probe reports for that endpoint.
    /// It is what lets the routing selector prove a report describes the
    /// checkpoint the panel actually played.
    tip_checkpoint_manifest_sha256: String,
    evidence_chain_sha256: String,
    sidecar_chain_sha256: Option<String>,
    prewindow_sidecar_chain_sha256: Option<String>,
    window_sidecars: Vec<Cycle4M3SidecarDigestV1>,
    decision_count: u64,
    cells: Vec<Cycle4M3CellV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3SidecarDigestV1 {
    pub update_index: u64,
    pub sha256: String,
}

impl Cycle4M3WindowV1 {
    #[must_use]
    pub const fn residual_mode(&self) -> Cycle4M3ResidualModeV1 {
        self.residual_mode
    }

    #[must_use]
    pub fn run_sha256(&self) -> &str {
        &self.run_sha256
    }

    #[must_use]
    pub const fn first_update_index(&self) -> u64 {
        self.first_update_index
    }

    #[must_use]
    pub const fn last_update_index(&self) -> u64 {
        self.last_update_index
    }

    #[must_use]
    pub const fn decision_count(&self) -> u64 {
        self.decision_count
    }

    #[must_use]
    pub fn cells(&self) -> &[Cycle4M3CellV1] {
        &self.cells
    }

    /// Repoints a test fixture's window at a different span, so a test can
    /// build a reference over an EARLIER snapshot of the same run without a
    /// second Store on disk.
    #[cfg(test)]
    pub(crate) fn set_window_bounds_for_test_v1(&mut self, first: u64, last: u64) {
        self.first_update_index = first;
        self.last_update_index = last;
    }
}

/// What to audit.
#[derive(Clone, Debug)]
pub struct Cycle4M3WindowRequestV1 {
    pub store_root: PathBuf,
    /// Required for [`Cycle4M3ResidualModeV1::Centered`], refused for
    /// [`Cycle4M3ResidualModeV1::Raw`] (a v3 Store has no chain).
    pub chain_dir: Option<PathBuf>,
    pub residual_mode: Cycle4M3ResidualModeV1,
    pub window_updates: u64,
}

fn aggregate_chain_digest_v1(label: &str, rows: &[(u64, [u8; 32])]) -> String {
    let mut bytes =
        Vec::with_capacity(CHAIN_DIGEST_DOMAIN_V1.len() + label.len() + rows.len() * 40);
    bytes.extend_from_slice(CHAIN_DIGEST_DOMAIN_V1);
    bytes.extend_from_slice(label.as_bytes());
    bytes.extend_from_slice(&(rows.len() as u64).to_be_bytes());
    for (index, digest) in rows {
        bytes.extend_from_slice(&index.to_be_bytes());
        bytes.extend_from_slice(digest);
    }
    lower_hex_raw32_v1(sha256_v1(&bytes))
}

fn role_from_wire_v1(value: &str) -> Result<BaselineRoleV4> {
    BaselineRoleV4::from_wire_v4(value).map_err(|_| {
        Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_invalid_learner_seat",
            format!("learner_seat {value} is neither p0 nor p1"),
        )
    })
}

fn parse_f32_bits_v1(value: &str, what: &str) -> Result<u32> {
    if value.len() != 8
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_invalid_f32_bits",
            format!("{what}: {value}"),
        ));
    }
    let bits = u32::from_str_radix(value, 16).map_err(|_| {
        Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_invalid_f32_bits",
            format!("{what}: {value}"),
        )
    })?;
    if !f32::from_bits(bits).is_finite() {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_invalid_f32_bits",
            format!("{what} is non-finite: {value}"),
        ));
    }
    Ok(bits)
}

/// One update's evidence, adapted into the per-episode cursor walk the v4
/// validator's view type expects: episodes in batch order, each carrying its
/// own contiguous slice of the flat `physical_terms` list, sized by that
/// episode's `learner_physical_decision_count`.
struct AdaptedUpdateV1 {
    terms: Vec<UpdateBaselineTermViewV4>,
    episode_spans: Vec<(BaselineRoleV4, i8, String, usize, usize)>,
}

impl AdaptedUpdateV1 {
    fn views_v1(&self) -> Vec<UpdateBaselineEpisodeViewV4<'_>> {
        self.episode_spans
            .iter()
            .map(
                |(role, learner_return, identity, start, end)| UpdateBaselineEpisodeViewV4 {
                    learner_seat: *role,
                    learner_return: *learner_return,
                    opponent_checkpoint_manifest_sha256: identity.as_str(),
                    terms: &self.terms[*start..*end],
                },
            )
            .collect()
    }
}

fn adapt_update_v1(path: &Path, evidence: &EvidenceReadV1) -> Result<AdaptedUpdateV1> {
    if evidence.schema != UPDATE_EVIDENCE_SCHEMA_V1 {
        return Err(decode_error_v1(
            path,
            format!("unexpected evidence schema {}", evidence.schema),
        ));
    }
    if evidence.learner_physical_decision_count != evidence.physical_terms.len() as u64 {
        return Err(decode_error_v1(
            path,
            format!(
                "update {} declares {} learner physical decisions but carries {} terms",
                evidence.update_index,
                evidence.learner_physical_decision_count,
                evidence.physical_terms.len()
            ),
        ));
    }
    let mut terms = Vec::with_capacity(evidence.physical_terms.len());
    for term in &evidence.physical_terms {
        terms.push(UpdateBaselineTermViewV4 {
            joint_log_probability_f32_bits: parse_f32_bits_v1(
                &term.joint_log_probability_f32_bits,
                "joint_log_probability_f32_bits",
            )?,
            value_f32_bits: parse_f32_bits_v1(&term.value_f32_bits, "value_f32_bits")?,
            terminal_return_i8: term.terminal_return_i8,
        });
    }
    let mut episode_spans = Vec::with_capacity(evidence.episodes.len());
    let mut cursor = 0_usize;
    for episode in &evidence.episodes {
        if episode.schema != EPISODE_SCHEMA_V1 {
            return Err(decode_error_v1(
                path,
                format!("unexpected episode schema {}", episode.schema),
            ));
        }
        let identity = match episode_cell_identity_v1(episode) {
            EpisodeCellIdentityV1::Identity(identity) => identity,
            EpisodeCellIdentityV1::Absent => {
                return Err(Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_episode_without_opponent_identity",
                    format!(
                        "{}: update {} has an episode with neither opponent_checkpoint_manifest_sha256 \
                         nor a search occupant identity (opponent_search_authority_sha256 plus \
                         opponent_search_tier); the M3 cell is (opponent identity, learner role) and \
                         cannot be formed for it",
                        path.display(),
                        evidence.update_index
                    ),
                ));
            }
            EpisodeCellIdentityV1::MalformedSearchOccupant(reason) => {
                return Err(Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_malformed_search_occupant_identity",
                    format!(
                        "{}: update {} has a search-occupant episode whose identity fields are \
                         malformed ({reason}); refused rather than hashed into a cell key",
                        path.display(),
                        evidence.update_index
                    ),
                ));
            }
        };
        let count = usize::try_from(episode.learner_physical_decision_count).map_err(|_| {
            decode_error_v1(
                path,
                "learner_physical_decision_count does not fit in usize",
            )
        })?;
        let end = cursor
            .checked_add(count)
            .ok_or_else(|| decode_error_v1(path, "learner physical decision cursor overflowed"))?;
        if end > terms.len() {
            return Err(decode_error_v1(
                path,
                format!(
                    "update {} episode cursor runs past the flat physical_terms list",
                    evidence.update_index
                ),
            ));
        }
        episode_spans.push((
            role_from_wire_v1(&episode.learner_seat)?,
            episode.learner_return,
            identity,
            cursor,
            end,
        ));
        cursor = end;
    }
    if cursor != terms.len() {
        return Err(decode_error_v1(
            path,
            format!(
                "update {} leaves {} unattributed physical terms",
                evidence.update_index,
                terms.len() - cursor
            ),
        ));
    }
    Ok(AdaptedUpdateV1 {
        terms,
        episode_spans,
    })
}

/// SHA-256 of the Store's own checkpoint manifest at `generation_index`.
///
/// `native_training_store_checkpoint_v3` sets a checkpoint's
/// `checkpoint_manifest_sha256` to `sha256(manifest canonical JSON)`, and the
/// leaf holds exactly those bytes, so hashing the file reproduces the
/// identity the M2 probe reports for the same checkpoint. A window whose last
/// update is not a checkpoint boundary has no such leaf and fails closed:
/// the audit is for a completed arm at its pinned endpoint.
fn tip_checkpoint_manifest_sha256_v1(store_root: &Path, generation_index: u64) -> Result<String> {
    let name = format!(
        "{CHECKPOINT_NAME_PREFIX_V1}{generation_index:0width$}{CHECKPOINT_NAME_SUFFIX_V1}",
        width = FIXED_INDEX_DIGITS_V1
    );
    let path = store_root.join(CHECKPOINT_DIRECTORY_V1).join(&name);
    let bytes = fs::read(&path).map_err(|error| {
        Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_tip_checkpoint_missing",
            format!(
                "{}: {error}; the audited window must end on a checkpoint boundary so the report \
                 can name the checkpoint identity the M2 panel plays",
                path.display()
            ),
        )
    })?;
    Ok(lower_hex_raw32_v1(sha256_v1(&bytes)))
}

#[cfg(test)]
thread_local! {
    /// Test-only fault injected between the audit's two filesystem passes.
    /// Thread-local, so tests running in parallel cannot see each other's.
    static BETWEEN_PASSES_FAULT_V1: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn run_between_passes_fault_v1() {
    BETWEEN_PASSES_FAULT_V1.with(|fault| {
        let fault = fault.borrow();
        if let Some(fault) = fault.as_ref() {
            fault();
        }
    });
}

#[cfg(not(test))]
const fn run_between_passes_fault_v1() {}

/// Installs a between-pass fault for the duration of the guard's lifetime.
#[cfg(test)]
struct BetweenPassesFaultGuardV1;

#[cfg(test)]
impl BetweenPassesFaultGuardV1 {
    fn install_v1(fault: impl Fn() + 'static) -> Self {
        BETWEEN_PASSES_FAULT_V1.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(fault));
        });
        Self
    }
}

#[cfg(test)]
impl Drop for BetweenPassesFaultGuardV1 {
    fn drop(&mut self) {
        BETWEEN_PASSES_FAULT_V1.with(|slot| {
            *slot.borrow_mut() = None;
        });
    }
}

fn sidecar_bytes_v1(chain_dir: &Path, update_index: u64) -> Result<Vec<u8>> {
    let name = format!("baseline-update-{update_index:08}.record.json");
    let path = chain_dir.join(&name);
    fs::read(&path).map_err(|error| {
        Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_sidecar_missing",
            format!("{}: {error}", path.display()),
        )
    })
}

/// Advances `prior` across one update using ONLY that update's sidecar, the
/// same rule the arm launcher replays in-boundary with: apply the sidecar's
/// own observations, require the declared `c_t` to equal the running state
/// and the declared `c_{t+1}` to equal the derived successor.
fn replay_sidecar_only_v1(
    prior: &NativeBaselineStateV4,
    chain_dir: &Path,
    update_index: u64,
) -> Result<(NativeBaselineStateV4, [u8; 32])> {
    let bytes = sidecar_bytes_v1(chain_dir, update_index)?;
    let digest = sha256_v1(&bytes);
    let record = decode_update_baseline_record_v4(&bytes).map_err(|error| {
        Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_sidecar_decode",
            format!("update {update_index}: {}", error.code()),
        )
    })?;
    if record.update_index() != update_index {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_sidecar_update_mismatch",
            format!(
                "sidecar at update {update_index} declares update {}",
                record.update_index()
            ),
        ));
    }
    let mut observations = Vec::with_capacity(record.cells().len());
    for cell in record.cells() {
        if prior.c_for_cell_v4(cell.key()).to_bits() != cell.c_t_bits() {
            return Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_sidecar_strict_lag",
                format!("update {update_index} declares a c_t the replayed chain never held"),
            ));
        }
        observations.push(BaselineObservationV4 {
            key: cell.key().clone(),
            residual_sum_f64: cell.residual_sum_f64(),
            decision_count: cell.decision_count(),
            episode_count: cell.episode_count(),
        });
    }
    let successor = prior.apply_update_v4(&observations).map_err(|error| {
        Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_sidecar_apply",
            format!("update {update_index}: {error}"),
        )
    })?;
    for cell in record.cells() {
        if successor.c_for_cell_v4(cell.key()).to_bits() != cell.c_next_bits() {
            return Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_sidecar_successor",
                format!(
                    "update {update_index} declares a c_next its own observations do not derive"
                ),
            ));
        }
    }
    Ok((successor, digest))
}

/// Reads the Store, walks the final `window_updates` updates, and returns
/// the per-cell table plus every input digest.
pub fn compute_cycle4_m3_window_v1(request: &Cycle4M3WindowRequestV1) -> Result<Cycle4M3WindowV1> {
    match (request.residual_mode, request.chain_dir.as_ref()) {
        (Cycle4M3ResidualModeV1::Centered, None) => {
            return Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_chain_dir_required",
                "the centered residual needs the arm's baseline chain directory",
            ))
        }
        (Cycle4M3ResidualModeV1::Raw, Some(_)) => {
            return Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_chain_dir_refused",
                "the raw reference residual is computed from a v3 Store, which has no chain",
            ))
        }
        _ => {}
    }
    if request.window_updates == 0 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_empty_window",
            "the window must span at least one update",
        ));
    }

    let files = list_continuation_paths_v1(&request.store_root)?;

    // Pass one: the update stream, proven contiguous and ascending, and its
    // declared evidence-digest chain walked end to end from the genesis
    // anchor. The chain binds each update's digest to its predecessor's, so a
    // spliced, reordered or truncated Store fails here before any residual is
    // read; pass two then proves each WINDOW digest against the evidence
    // bytes it claims to cover.
    let mut file_ranges: Vec<(PathBuf, u64)> = Vec::with_capacity(files.len());
    let mut expected: Option<u64> = None;
    let mut first_update: Option<u64> = None;
    let mut previous_declared_digest: Option<String> = None;
    let mut declared_digests: BTreeMap<u64, [u8; 32]> = BTreeMap::new();
    for (_, _, path) in &files {
        let bytes = fs::read(path).map_err(|error| io_error_v1(path, &error))?;
        let indexed: ContinuationIndexReadV1 =
            serde_json::from_slice(&bytes).map_err(|error| decode_error_v1(path, error))?;
        if indexed.update_groups.is_empty() {
            continue;
        }
        for group in &indexed.update_groups {
            match expected {
                None => {
                    first_update = Some(group.update_index);
                }
                Some(next) if next == group.update_index => {}
                Some(next) => {
                    return Err(Cycle4M3AuditErrorV1::new(
                        "cycle4_m3_audit_v1_noncontiguous_updates",
                        format!(
                            "{}: expected update {next}, found {}",
                            path.display(),
                            group.update_index
                        ),
                    ))
                }
            }
            // The first update in the Store must be the genesis anchor (no
            // predecessor); every later one must name its predecessor's own
            // declared digest.
            if group.previous_update_evidence_sha256 != previous_declared_digest {
                return Err(Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_evidence_chain_broken",
                    format!(
                        "{}: update {} names predecessor {:?} where the walked chain holds {:?}",
                        path.display(),
                        group.update_index,
                        group.previous_update_evidence_sha256,
                        previous_declared_digest
                    ),
                ));
            }
            let digest = parse_lower_hex_raw32_v1(&group.update_evidence_sha256).map_err(|_| {
                decode_error_v1(
                    path,
                    format!(
                        "update {} carries a malformed update_evidence_sha256",
                        group.update_index
                    ),
                )
            })?;
            declared_digests.insert(group.update_index, digest);
            previous_declared_digest = Some(group.update_evidence_sha256.clone());
            expected = Some(group.update_index + 1);
        }
        let end = expected.expect("at least one group") - 1;
        file_ranges.push((path.clone(), end));
    }
    let (Some(first_update), Some(next_after_tip)) = (first_update, expected) else {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_empty_store",
            format!("{} carries no update groups", request.store_root.display()),
        ));
    };
    let tip_update = next_after_tip - 1;
    let available = tip_update - first_update + 1;
    if available < request.window_updates {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_window_too_short",
            format!(
                "the Store holds {available} updates, fewer than the pinned window of {}",
                request.window_updates
            ),
        ));
    }
    let window_first = tip_update - request.window_updates + 1;

    // Injected fault point, `cfg(test)` only and a no-op otherwise: the
    // between-pass guard above can only be exercised by a Store that really
    // does change between the two reads, and nothing else in this module can
    // arrange that. Mirrors the established
    // `DurablePublicationErrorKindV1::InjectedFault` precedent.
    run_between_passes_fault_v1();

    // Pre-window replay: sidecar-only, from the chain's empty genesis state.
    let mut prior = NativeBaselineStateV4::empty_v4();
    let mut prewindow_rows: Vec<(u64, [u8; 32])> = Vec::new();
    if let Some(chain_dir) = request.chain_dir.as_ref() {
        for update_index in first_update..window_first {
            let (successor, digest) = replay_sidecar_only_v1(&prior, chain_dir, update_index)?;
            prior = successor;
            prewindow_rows.push((update_index, digest));
        }
    }

    // Pass two: the window itself.
    let mut accumulators: BTreeMap<BaselineCellKeyV4, CellAccumulatorV1> = BTreeMap::new();
    let mut episode_counts: BTreeMap<BaselineCellKeyV4, u64> = BTreeMap::new();
    let mut evidence_rows: Vec<(u64, [u8; 32])> = Vec::new();
    let mut sidecar_rows: Vec<(u64, [u8; 32])> = Vec::new();
    let mut window_sidecars: Vec<Cycle4M3SidecarDigestV1> = Vec::new();
    let mut run_sha256: Option<String> = None;
    let mut tip_evidence_sha256: Option<String> = None;
    let mut decision_count = 0_u64;
    let mut seen = window_first;

    for (path, end) in &file_ranges {
        if *end < window_first {
            continue;
        }
        let bytes = fs::read(path).map_err(|error| io_error_v1(path, &error))?;
        // The canonical decode is the container-level integrity check: it
        // enforces the byte-exact canonical form and the Store's own three
        // nullable paths, and it yields the untyped tree the per-update
        // evidence bytes are re-encoded from.
        let document: serde_json::Value =
            from_canonical_json_bytes_v1(&bytes, M3_CONTINUATION_NULL_POLICY_V1).map_err(
                |error| {
                    Cycle4M3AuditErrorV1::new(
                        "cycle4_m3_audit_v1_continuation_not_canonical",
                        format!("{}: {error}", path.display()),
                    )
                },
            )?;
        let continuation: ContinuationReadV1 =
            serde_json::from_slice(&bytes).map_err(|error| decode_error_v1(path, error))?;
        for (ordinal, group) in continuation.update_groups.iter().enumerate() {
            if group.update_index < window_first {
                continue;
            }
            if group.update_index != seen {
                return Err(Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_noncontiguous_updates",
                    format!(
                        "{}: expected update {seen}, found {}",
                        path.display(),
                        group.update_index
                    ),
                ));
            }
            seen += 1;
            if group.evidence.update_index != group.update_index {
                return Err(decode_error_v1(
                    path,
                    format!(
                        "group {} embeds evidence for update {}",
                        group.update_index, group.evidence.update_index
                    ),
                ));
            }
            match &run_sha256 {
                None => run_sha256 = Some(group.evidence.run_sha256.clone()),
                Some(existing) if *existing == group.evidence.run_sha256 => {}
                Some(existing) => {
                    return Err(Cycle4M3AuditErrorV1::new(
                        "cycle4_m3_audit_v1_run_identity_drift",
                        format!(
                            "update {} declares run {} where the window opened on {existing}",
                            group.update_index, group.evidence.run_sha256
                        ),
                    ))
                }
            }
            let evidence_digest =
                parse_lower_hex_raw32_v1(&group.update_evidence_sha256).map_err(|_| {
                    decode_error_v1(
                        path,
                        format!(
                            "update {} carries a malformed update_evidence_sha256",
                            group.update_index
                        ),
                    )
                })?;
            // The two filesystem passes must have read the SAME Store. Pass
            // one walked the declared digest chain from the genesis anchor;
            // pass two re-reads the files, so a leaf replaced in between with
            // altered evidence and a freshly recomputed, internally
            // consistent digest would otherwise be accepted -- and the TIP
            // update is entirely free that way, since no later record links
            // to it. Every second-pass digest is therefore compared against
            // the value the chain walk fixed.
            let walked_digest = declared_digests
                .get(&group.update_index)
                .copied()
                .ok_or_else(|| {
                    Cycle4M3AuditErrorV1::new(
                        "cycle4_m3_audit_v1_evidence_replaced_between_passes",
                        format!(
                            "{}: update {} was not present when the chain was walked",
                            path.display(),
                            group.update_index
                        ),
                    )
                })?;
            if walked_digest != evidence_digest {
                return Err(Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_evidence_replaced_between_passes",
                    format!(
                        "{}: update {} declared {} when the chain was walked and {} when its \
                         evidence was read",
                        path.display(),
                        group.update_index,
                        lower_hex_raw32_v1(walked_digest),
                        group.update_evidence_sha256
                    ),
                ));
            }
            evidence_rows.push((group.update_index, evidence_digest));
            tip_evidence_sha256 = Some(group.update_evidence_sha256.clone());

            let adapted = adapt_update_v1(path, &group.evidence)?;
            let views = adapted.views_v1();

            // Digest recomputation. Everything above this line is a
            // DECLARATION; this is where the declaration is made to answer
            // for the evidence bytes it covers. Without it, an edit that
            // preserves every sidecar quantity (two equal-policy-weight
            // values in one cell moved in opposite directions) would change
            // the cell's sample standard deviation and pass unnoticed.
            let evidence_value = document
                .get("update_groups")
                .and_then(|groups| groups.get(ordinal))
                .and_then(|group| group.get("evidence"))
                .ok_or_else(|| {
                    decode_error_v1(
                        path,
                        format!(
                            "update {} has no evidence object in the canonical tree",
                            group.update_index
                        ),
                    )
                })?;
            let evidence_cj =
                to_canonical_json_bytes_v1(evidence_value, M3_EVIDENCE_NULL_POLICY_V1).map_err(
                    |error| {
                        Cycle4M3AuditErrorV1::new(
                            "cycle4_m3_audit_v1_evidence_digest_recompute",
                            format!("update {}: {error}", group.update_index),
                        )
                    },
                )?;
            let declared_run =
                parse_lower_hex_raw32_v1(&group.evidence.run_sha256).map_err(|_| {
                    decode_error_v1(
                        path,
                        format!(
                            "update {} carries a malformed evidence run_sha256",
                            group.update_index
                        ),
                    )
                })?;
            let previous_digest = if group.update_index == first_update {
                None
            } else {
                Some(
                    *declared_digests
                        .get(&(group.update_index - 1))
                        .ok_or_else(|| {
                            Cycle4M3AuditErrorV1::new(
                                "cycle4_m3_audit_v1_evidence_chain_broken",
                                format!("update {} has no predecessor digest", group.update_index),
                            )
                        })?,
                )
            };
            // The two mirrors of the same file must agree about the chain
            // link before it is hashed into the digest.
            if group.previous_update_evidence_sha256 != previous_digest.map(lower_hex_raw32_v1) {
                return Err(Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_evidence_chain_broken",
                    format!(
                        "{}: update {} names a predecessor the walked chain does not hold",
                        path.display(),
                        group.update_index
                    ),
                ));
            }
            let recomputed = recompute_update_evidence_sha256_v1(
                declared_run,
                group.update_index,
                previous_digest,
                &evidence_cj,
            )?;
            if recomputed != evidence_digest {
                return Err(Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_evidence_digest_mismatch",
                    format!(
                        "{}: update {} declares update_evidence_sha256 {} but its own evidence \
                         bytes hash to {}",
                        path.display(),
                        group.update_index,
                        group.update_evidence_sha256,
                        lower_hex_raw32_v1(recomputed)
                    ),
                ));
            }

            // Recover `c_t` exactly as the v4 evidence validator does: the
            // validator itself proves the sidecar against this update's own
            // evidence and the running prior state, and returns the
            // successor. Nothing here reinterprets it.
            let successor = if let Some(chain_dir) = request.chain_dir.as_ref() {
                let sidecar = sidecar_bytes_v1(chain_dir, group.update_index)?;
                let sidecar_digest = sha256_v1(&sidecar);
                let record = decode_update_baseline_record_v4(&sidecar).map_err(|error| {
                    Cycle4M3AuditErrorV1::new(
                        "cycle4_m3_audit_v1_sidecar_decode",
                        format!("update {}: {}", group.update_index, error.code()),
                    )
                })?;
                let successor = validate_update_baseline_v4(
                    &views,
                    &record,
                    &prior,
                    group.update_index,
                    evidence_digest,
                )
                .map_err(|error| {
                    Cycle4M3AuditErrorV1::new(
                        "cycle4_m3_audit_v1_sidecar_validation",
                        format!("update {}: {}", group.update_index, error.code()),
                    )
                })?;
                sidecar_rows.push((group.update_index, sidecar_digest));
                window_sidecars.push(Cycle4M3SidecarDigestV1 {
                    update_index: group.update_index,
                    sha256: lower_hex_raw32_v1(sidecar_digest),
                });
                Some(successor)
            } else {
                None
            };

            for view in &views {
                let key = BaselineCellKeyV4::new_v4(
                    view.opponent_checkpoint_manifest_sha256,
                    view.learner_seat,
                )
                .map_err(|_| {
                    Cycle4M3AuditErrorV1::new(
                        "cycle4_m3_audit_v1_invalid_cell_identity",
                        format!(
                            "update {} carries a malformed opponent checkpoint identity",
                            group.update_index
                        ),
                    )
                })?;
                let centering = match request.residual_mode {
                    Cycle4M3ResidualModeV1::Centered => prior.c_for_cell_v4(&key),
                    Cycle4M3ResidualModeV1::Raw => 0.0_f32,
                };
                *episode_counts.entry(key.clone()).or_insert(0) += 1;
                let accumulator = accumulators.entry(key).or_default();
                for term in view.terms {
                    if term.terminal_return_i8 != view.learner_return {
                        return Err(Cycle4M3AuditErrorV1::new(
                            "cycle4_m3_audit_v1_term_return_mismatch",
                            format!(
                                "update {} has a term whose terminal return disagrees with its \
                                 episode",
                                group.update_index
                            ),
                        ));
                    }
                    let target = f32::from(term.terminal_return_i8);
                    let value = f32::from_bits(term.value_f32_bits);
                    let residual = target - value;
                    let centered = residual - centering;
                    if !centered.is_finite() {
                        return Err(Cycle4M3AuditErrorV1::new(
                            "cycle4_m3_audit_v1_nonfinite_residual",
                            format!(
                                "update {} produced a non-finite residual",
                                group.update_index
                            ),
                        ));
                    }
                    accumulator.observe_v1(f64::from(centered));
                    decision_count += 1;
                }
            }

            if let Some(successor) = successor {
                prior = successor;
            }
        }
    }

    if seen != tip_update + 1 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_window_incomplete",
            format!("walked to update {seen} but the Store tip is {tip_update}"),
        ));
    }

    let cells = accumulators
        .iter()
        .map(|(key, accumulator)| Cycle4M3CellV1 {
            opponent_checkpoint_manifest_sha256: key.opponent_checkpoint_manifest_sha256.clone(),
            role: key.role.wire_v4().to_owned(),
            decision_count: accumulator.count,
            episode_count: episode_counts.get(key).copied().unwrap_or(0),
            mean_residual: RealV1::from_f64_v1(accumulator.mean),
            sample_standard_deviation: RealV1::from_f64_v1(
                accumulator.sample_standard_deviation_v1(),
            ),
            qualifies: accumulator.count >= CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1,
        })
        .collect::<Vec<_>>();

    Ok(Cycle4M3WindowV1 {
        residual_mode: request.residual_mode,
        run_sha256: run_sha256.unwrap_or_default(),
        first_update_index: window_first,
        last_update_index: tip_update,
        tip_update_evidence_sha256: tip_evidence_sha256.unwrap_or_default(),
        tip_checkpoint_manifest_sha256: tip_checkpoint_manifest_sha256_v1(
            &request.store_root,
            tip_update,
        )?,
        evidence_chain_sha256: aggregate_chain_digest_v1("window-evidence", &evidence_rows),
        sidecar_chain_sha256: request
            .chain_dir
            .as_ref()
            .map(|_| aggregate_chain_digest_v1("window-sidecar", &sidecar_rows)),
        prewindow_sidecar_chain_sha256: request
            .chain_dir
            .as_ref()
            .map(|_| aggregate_chain_digest_v1("prewindow-sidecar", &prewindow_rows)),
        window_sidecars,
        decision_count,
        cells,
    })
}

// ---------------------------------------------------------------------
// The gate (pure)
// ---------------------------------------------------------------------

/// The amendment's total function, evaluated over an already-computed cell
/// table. Pure: no I/O, no store, no chain.
#[derive(Clone, Debug, PartialEq)]
pub struct Cycle4M3GateV1 {
    pub qualifying_cell_count: u64,
    pub qualifying_decision_count: u64,
    pub window_decision_count: u64,
    pub max_abs_mean: Option<f64>,
    pub max_abs_mean_cell: Option<(String, String)>,
    pub decision_weighted_mean_standard_deviation: Option<f64>,
    pub reference_decision_weighted_mean_standard_deviation: f64,
    pub dispersion_allowance: f64,
    pub verdict_pass: bool,
    pub failures: Vec<&'static str>,
}

/// Evaluates section A over `cells`. `reference_dispersion` is the same
/// statistic computed from the cycle-3 focal store's final 512 updates on
/// the RAW residual.
///
/// Failure codes are stable strings recorded verbatim in the report:
/// `no_cell_qualifies`, `coverage_below_floor`, `centering_above_threshold`,
/// `dispersion_above_allowance`. Every applicable clause is evaluated, so a
/// report names every reason it failed rather than only the first.
#[must_use]
pub fn evaluate_cycle4_m3_gate_v1(
    cells: &[Cycle4M3CellV1],
    window_decision_count: u64,
    reference_dispersion: f64,
) -> Cycle4M3GateV1 {
    let qualifying: Vec<&Cycle4M3CellV1> = cells.iter().filter(|cell| cell.qualifies).collect();
    let qualifying_decision_count: u64 = qualifying.iter().map(|cell| cell.decision_count).sum();
    let dispersion_allowance = CYCLE4_M3_DISPERSION_RATIO_MAX_V1 * reference_dispersion;
    let mut failures = Vec::new();

    if qualifying.is_empty() {
        failures.push("no_cell_qualifies");
        return Cycle4M3GateV1 {
            qualifying_cell_count: 0,
            qualifying_decision_count: 0,
            window_decision_count,
            max_abs_mean: None,
            max_abs_mean_cell: None,
            decision_weighted_mean_standard_deviation: None,
            reference_decision_weighted_mean_standard_deviation: reference_dispersion,
            dispersion_allowance,
            verdict_pass: false,
            failures,
        };
    }

    // Exact integer coverage: qualifying/total < 80/100.
    if u128::from(qualifying_decision_count) * u128::from(100 - CYCLE4_M3_COVERAGE_FLOOR_PERCENT_V1)
        < u128::from(window_decision_count - qualifying_decision_count)
            * u128::from(CYCLE4_M3_COVERAGE_FLOOR_PERCENT_V1)
    {
        failures.push("coverage_below_floor");
    }

    let mut max_abs_mean = 0.0_f64;
    let mut max_abs_mean_cell: Option<(String, String)> = None;
    for cell in &qualifying {
        // `to_f64_v1` cannot fail on a table this module built; a table
        // decoded from a document was validated when it was decoded.
        let mean = cell.mean_residual.to_f64_v1().unwrap_or(f64::NAN);
        let magnitude = mean.abs();
        if max_abs_mean_cell.is_none() || magnitude > max_abs_mean {
            max_abs_mean = magnitude;
            max_abs_mean_cell = Some((
                cell.opponent_checkpoint_manifest_sha256.clone(),
                cell.role.clone(),
            ));
        }
    }
    // Written as an explicit finiteness check plus `>` rather than
    // `!(x <= threshold)`: a NaN statistic must FAIL, and `>` alone would
    // silently pass it.
    if !max_abs_mean.is_finite() || max_abs_mean > CYCLE4_M3_CENTERING_MAX_ABS_MEAN_V1 {
        failures.push("centering_above_threshold");
    }

    let dispersion = decision_weighted_mean_standard_deviation_v1(&qualifying);
    if !dispersion.is_finite()
        || !dispersion_allowance.is_finite()
        || dispersion > dispersion_allowance
    {
        failures.push("dispersion_above_allowance");
    }

    Cycle4M3GateV1 {
        qualifying_cell_count: qualifying.len() as u64,
        qualifying_decision_count,
        window_decision_count,
        max_abs_mean: Some(max_abs_mean),
        max_abs_mean_cell,
        decision_weighted_mean_standard_deviation: Some(dispersion),
        reference_decision_weighted_mean_standard_deviation: reference_dispersion,
        dispersion_allowance,
        verdict_pass: failures.is_empty(),
        failures,
    }
}

/// "the decision-weighted mean over qualifying cells of the per-cell sample
/// standard deviation (weights = the cell's decision count; denominator =
/// the sum of weights)". Summed in the table's own canonical cell order.
#[allow(clippy::cast_precision_loss)]
fn decision_weighted_mean_standard_deviation_v1(cells: &[&Cycle4M3CellV1]) -> f64 {
    let mut weighted = 0.0_f64;
    let mut weights = 0.0_f64;
    for cell in cells {
        let weight = cell.decision_count as f64;
        weighted += weight
            * cell
                .sample_standard_deviation
                .to_f64_v1()
                .unwrap_or(f64::NAN);
        weights += weight;
    }
    if weights == 0.0 {
        return 0.0;
    }
    weighted / weights
}

// ---------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3WindowWireV1 {
    pub first_update_index: u64,
    pub last_update_index: u64,
    pub update_count: u64,
}

fn validate_reference_window_v1(window: &Cycle4M3WindowWireV1) -> Result<()> {
    let internally_consistent_count = window
        .last_update_index
        .checked_sub(window.first_update_index)
        .and_then(|span| span.checked_add(1));
    if internally_consistent_count != Some(window.update_count) {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_window",
            format!(
                "the reference window declares {} updates for bounds {} through {}",
                window.update_count, window.first_update_index, window.last_update_index
            ),
        ));
    }
    if window.first_update_index != CYCLE4_M3_REFERENCE_WINDOW_FIRST_UPDATE_INDEX_V1
        || window.last_update_index != CYCLE4_M3_REFERENCE_WINDOW_LAST_UPDATE_INDEX_V1
        || window.update_count != CYCLE4_M3_WINDOW_UPDATES_V1
    {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_window",
            format!(
                "the reference window must be updates {} through {} with count {}, not {} through {} with count {}",
                CYCLE4_M3_REFERENCE_WINDOW_FIRST_UPDATE_INDEX_V1,
                CYCLE4_M3_REFERENCE_WINDOW_LAST_UPDATE_INDEX_V1,
                CYCLE4_M3_WINDOW_UPDATES_V1,
                window.first_update_index,
                window.last_update_index,
                window.update_count
            ),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3TotalsWireV1 {
    pub window_decision_count: u64,
    pub qualifying_cell_count: u64,
    pub qualifying_decision_count: u64,
    pub cell_count: u64,
}

/// The reference document: the RAW-residual dispersion statistic computed
/// from the cycle-3 focal store's final 512 updates, published so the audit
/// binds one immutable artifact by hash.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3ReferenceDocumentV1 {
    pub schema: String,
    pub residual_mode: String,
    pub run_sha256: String,
    pub window: Cycle4M3WindowWireV1,
    pub tip_update_evidence_sha256: String,
    pub tip_checkpoint_manifest_sha256: String,
    pub evidence_chain_sha256: String,
    /// SHA-256 of the ratified audit note's bytes
    /// (`OX_ADVANTAGE_BY_ROLE_AUDIT_RESULT_V1.md`). REQUIRED: clarification
    /// V2.1 binds the note's bytes into the reference, so the audit bin's
    /// `--audit-note` is required in reference mode and the routing selector
    /// refuses a report whose reference did not carry one. The note records
    /// means and winrates, not per-cell standard deviations, so it cannot
    /// itself supply this statistic; see the README.
    pub audit_note_sha256: String,
    pub totals: Cycle4M3TotalsWireV1,
    pub cells: Vec<Cycle4M3CellV1>,
    pub decision_weighted_mean_standard_deviation: RealV1,
}

/// Builds the reference document's canonical bytes from a RAW window.
pub fn build_cycle4_m3_reference_document_v1(
    window: &Cycle4M3WindowV1,
    audit_note_sha256: String,
) -> Result<Vec<u8>> {
    if window.residual_mode != Cycle4M3ResidualModeV1::Raw {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_mode",
            "the reference statistic is computed on the RAW residual",
        ));
    }
    let reference_window = Cycle4M3WindowWireV1 {
        first_update_index: window.first_update_index,
        last_update_index: window.last_update_index,
        update_count: window
            .last_update_index
            .checked_sub(window.first_update_index)
            .and_then(|span| span.checked_add(1))
            .ok_or_else(|| {
                Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_reference_window",
                    format!(
                        "the reference window bounds {} through {} are invalid",
                        window.first_update_index, window.last_update_index
                    ),
                )
            })?,
    };
    validate_reference_window_v1(&reference_window)?;
    let qualifying: Vec<&Cycle4M3CellV1> =
        window.cells.iter().filter(|cell| cell.qualifies).collect();
    if qualifying.is_empty() {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_no_qualifying_cell",
            "no reference cell holds the qualifying decision count, so the reference \
             statistic has no population",
        ));
    }
    let document = Cycle4M3ReferenceDocumentV1 {
        schema: CYCLE4_M3_REFERENCE_SCHEMA_V2.to_owned(),
        residual_mode: window.residual_mode.wire_v1().to_owned(),
        run_sha256: window.run_sha256.clone(),
        window: reference_window,
        tip_update_evidence_sha256: window.tip_update_evidence_sha256.clone(),
        tip_checkpoint_manifest_sha256: window.tip_checkpoint_manifest_sha256.clone(),
        evidence_chain_sha256: window.evidence_chain_sha256.clone(),
        audit_note_sha256,
        totals: Cycle4M3TotalsWireV1 {
            window_decision_count: window.decision_count,
            qualifying_cell_count: qualifying.len() as u64,
            qualifying_decision_count: qualifying.iter().map(|cell| cell.decision_count).sum(),
            cell_count: window.cells.len() as u64,
        },
        cells: window.cells.clone(),
        decision_weighted_mean_standard_deviation: RealV1::from_f64_v1(
            decision_weighted_mean_standard_deviation_v1(&qualifying),
        ),
    };
    to_canonical_json_bytes_v1(&document, CanonicalJsonNullPolicyV1::Forbid).map_err(|error| {
        Cycle4M3AuditErrorV1::new("cycle4_m3_audit_v1_canonical_json", error.to_string())
    })
}

/// Every total, derived from the cell table and nothing else.
///
/// `window_decision_count` is the CHECKED sum of every cell's decision count,
/// never the declared value: every learner physical term belongs to exactly
/// one cell, so the sum is the window's decision count by construction, and
/// copying the declaration would have let an edited denominator move the
/// coverage floor.
fn recompute_totals_v1(
    cells: &[Cycle4M3CellV1],
    qualifying: &[&Cycle4M3CellV1],
) -> Result<Cycle4M3TotalsWireV1> {
    fn checked_sum_v1(rows: impl Iterator<Item = u64>) -> Result<u64> {
        let mut total = 0_u64;
        for count in rows {
            total = total.checked_add(count).ok_or_else(|| {
                Cycle4M3AuditErrorV1::new(
                    "cycle4_m3_audit_v1_totals_overflow",
                    "a decision count sum overflowed u64",
                )
            })?;
        }
        Ok(total)
    }
    Ok(Cycle4M3TotalsWireV1 {
        window_decision_count: checked_sum_v1(cells.iter().map(|cell| cell.decision_count))?,
        qualifying_cell_count: qualifying.len() as u64,
        qualifying_decision_count: checked_sum_v1(
            qualifying.iter().map(|cell| cell.decision_count),
        )?,
        cell_count: cells.len() as u64,
    })
}

/// Decodes and structurally validates a reference document.
pub fn decode_cycle4_m3_reference_document_v1(bytes: &[u8]) -> Result<Cycle4M3ReferenceDocumentV1> {
    let schema = decode_cycle4_m3_schema_identity_v1(bytes, "cycle4_m3_audit_v1_reference_schema")?;
    if schema == CYCLE4_M3_REFERENCE_SCHEMA_LEGACY_V1 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_schema_v1_unsupported",
            format!(
                "schema {schema} names the obsolete reference-document layout; expected {}",
                CYCLE4_M3_REFERENCE_SCHEMA_V2
            ),
        ));
    }
    if schema != CYCLE4_M3_REFERENCE_SCHEMA_V2 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_schema",
            format!("unexpected schema {schema}"),
        ));
    }
    let document: Cycle4M3ReferenceDocumentV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid).map_err(
            |error| {
                Cycle4M3AuditErrorV1::new("cycle4_m3_audit_v1_canonical_json", error.to_string())
            },
        )?;
    debug_assert_eq!(document.schema, CYCLE4_M3_REFERENCE_SCHEMA_V2);
    if Cycle4M3ResidualModeV1::from_wire_v1(&document.residual_mode)? != Cycle4M3ResidualModeV1::Raw
    {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_mode",
            "the reference statistic must be the RAW residual",
        ));
    }
    validate_reference_window_v1(&document.window)?;
    if document.audit_note_sha256.len() != 64 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_audit_note",
            "the reference must bind the ratified audit note's bytes by SHA-256",
        ));
    }
    for cell in &document.cells {
        cell.mean_residual.to_f64_v1()?;
        cell.sample_standard_deviation.to_f64_v1()?;
        if cell.qualifies != (cell.decision_count >= CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1) {
            return Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_reference_cell_qualification",
                "a reference cell's declared qualification disagrees with its decision count",
            ));
        }
    }
    // The reference statistic is the number the whole dispersion clause is
    // measured against, so it is never accepted as a declaration: it is
    // recomputed from the document's own cell table, along with the totals
    // that decide which cells enter it, and must match bit for bit.
    let qualifying: Vec<&Cycle4M3CellV1> = document
        .cells
        .iter()
        .filter(|cell| cell.qualifies)
        .collect();
    if qualifying.is_empty() {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_no_qualifying_cell",
            "no reference cell qualifies, so the reference statistic has no population",
        ));
    }
    let recomputed_totals = recompute_totals_v1(&document.cells, &qualifying)?;
    if recomputed_totals != document.totals {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_totals_mismatch",
            "the reference totals do not follow from its own cell table",
        ));
    }
    let recomputed = RealV1::from_f64_v1(decision_weighted_mean_standard_deviation_v1(&qualifying));
    if recomputed.f64_bits != document.decision_weighted_mean_standard_deviation.f64_bits {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_reference_statistic_mismatch",
            format!(
                "the reference declares {} but its own cell table gives {}",
                document.decision_weighted_mean_standard_deviation.f64_bits, recomputed.f64_bits
            ),
        ));
    }
    Ok(document)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3ThresholdsWireV1 {
    pub qualifying_min_decisions: u64,
    pub coverage_floor_percent: u64,
    pub centering_max_abs_mean: RealV1,
    pub dispersion_ratio_max: RealV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3InputsWireV1 {
    pub run_sha256: String,
    pub tip_update_evidence_sha256: String,
    /// The identity the M2 probe reports for this endpoint's checkpoint. The
    /// routing selector requires it to equal the panel endpoint's, so a
    /// report from another run or an earlier tip cannot set eligibility.
    pub tip_checkpoint_manifest_sha256: String,
    pub evidence_chain_sha256: String,
    pub sidecar_chain_sha256: String,
    pub prewindow_sidecar_chain_sha256: String,
    pub window_sidecars: Vec<Cycle4M3SidecarDigestV1>,
    pub reference_document_sha256: String,
    pub reference_run_sha256: String,
    /// The reference Store's own tip checkpoint identity. Run identity alone
    /// does not pin WHICH snapshot of that run was measured: an older store
    /// ending at update 1536 shares the run and would supply a different
    /// 512-update reference, so the selector requires this to equal
    /// `--cycle3-g2048-checkpoint-manifest-sha256`.
    pub reference_tip_checkpoint_manifest_sha256: String,
    pub reference_window: Cycle4M3WindowWireV1,
    pub reference_audit_note_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3StatisticsWireV1 {
    pub max_abs_mean_residual: RealV1,
    pub max_abs_mean_cell_identity: String,
    pub max_abs_mean_cell_role: String,
    pub decision_weighted_mean_standard_deviation: RealV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3ReportReferenceWireV1 {
    pub decision_weighted_mean_standard_deviation: RealV1,
    pub dispersion_allowance: RealV1,
}

/// The audit report. `verdict` is `"PASS"` or `"FAIL"`; `failures` names
/// every clause that failed and is empty exactly when the verdict is PASS.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle4M3AuditReportV1 {
    pub schema: String,
    pub arm_kind: String,
    pub residual_mode: String,
    pub window: Cycle4M3WindowWireV1,
    pub inputs: Cycle4M3InputsWireV1,
    pub thresholds: Cycle4M3ThresholdsWireV1,
    pub totals: Cycle4M3TotalsWireV1,
    pub cells: Vec<Cycle4M3CellV1>,
    pub reference: Cycle4M3ReportReferenceWireV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statistics: Option<Cycle4M3StatisticsWireV1>,
    pub verdict: String,
    pub failures: Vec<String>,
}

pub const CYCLE4_M3_VERDICT_PASS_V1: &str = "PASS";
pub const CYCLE4_M3_VERDICT_FAIL_V1: &str = "FAIL";

fn decode_cycle4_m3_schema_identity_v1(bytes: &[u8], missing_code: &'static str) -> Result<String> {
    let document: serde_json::Value =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid).map_err(
            |error| {
                Cycle4M3AuditErrorV1::new("cycle4_m3_audit_v1_canonical_json", error.to_string())
            },
        )?;
    document
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            Cycle4M3AuditErrorV1::new(missing_code, "the document schema must be a string")
        })
}

/// Builds the audit report's canonical bytes from a CENTERED window plus a
/// decoded reference document (and that document's own SHA-256).
pub fn build_cycle4_m3_audit_report_v1(
    arm_kind: &str,
    window: &Cycle4M3WindowV1,
    reference: &Cycle4M3ReferenceDocumentV1,
    reference_document_sha256: &str,
) -> Result<(Vec<u8>, bool)> {
    if window.residual_mode != Cycle4M3ResidualModeV1::Centered {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_audit_mode",
            "the eligibility gate is computed on the CENTERED residual",
        ));
    }
    let reference_dispersion = reference
        .decision_weighted_mean_standard_deviation
        .to_f64_v1()?;
    let gate =
        evaluate_cycle4_m3_gate_v1(&window.cells, window.decision_count, reference_dispersion);
    let statistics = match (
        gate.max_abs_mean,
        gate.decision_weighted_mean_standard_deviation,
    ) {
        (Some(max_abs_mean), Some(dispersion)) => {
            let (identity, role) = gate
                .max_abs_mean_cell
                .clone()
                .unwrap_or_else(|| (String::new(), String::new()));
            Some(Cycle4M3StatisticsWireV1 {
                max_abs_mean_residual: RealV1::from_f64_v1(max_abs_mean),
                max_abs_mean_cell_identity: identity,
                max_abs_mean_cell_role: role,
                decision_weighted_mean_standard_deviation: RealV1::from_f64_v1(dispersion),
            })
        }
        _ => None,
    };
    let report = Cycle4M3AuditReportV1 {
        schema: CYCLE4_M3_AUDIT_SCHEMA_V2.to_owned(),
        arm_kind: arm_kind.to_owned(),
        residual_mode: window.residual_mode.wire_v1().to_owned(),
        window: Cycle4M3WindowWireV1 {
            first_update_index: window.first_update_index,
            last_update_index: window.last_update_index,
            update_count: window.last_update_index - window.first_update_index + 1,
        },
        inputs: Cycle4M3InputsWireV1 {
            run_sha256: window.run_sha256.clone(),
            tip_update_evidence_sha256: window.tip_update_evidence_sha256.clone(),
            tip_checkpoint_manifest_sha256: window.tip_checkpoint_manifest_sha256.clone(),
            evidence_chain_sha256: window.evidence_chain_sha256.clone(),
            sidecar_chain_sha256: window.sidecar_chain_sha256.clone().unwrap_or_default(),
            prewindow_sidecar_chain_sha256: window
                .prewindow_sidecar_chain_sha256
                .clone()
                .unwrap_or_default(),
            window_sidecars: window.window_sidecars.clone(),
            reference_document_sha256: reference_document_sha256.to_owned(),
            reference_run_sha256: reference.run_sha256.clone(),
            reference_tip_checkpoint_manifest_sha256: reference
                .tip_checkpoint_manifest_sha256
                .clone(),
            reference_window: reference.window.clone(),
            reference_audit_note_sha256: reference.audit_note_sha256.clone(),
        },
        thresholds: Cycle4M3ThresholdsWireV1 {
            qualifying_min_decisions: CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1,
            coverage_floor_percent: CYCLE4_M3_COVERAGE_FLOOR_PERCENT_V1,
            centering_max_abs_mean: RealV1::from_f64_v1(CYCLE4_M3_CENTERING_MAX_ABS_MEAN_V1),
            dispersion_ratio_max: RealV1::from_f64_v1(CYCLE4_M3_DISPERSION_RATIO_MAX_V1),
        },
        totals: Cycle4M3TotalsWireV1 {
            window_decision_count: gate.window_decision_count,
            qualifying_cell_count: gate.qualifying_cell_count,
            qualifying_decision_count: gate.qualifying_decision_count,
            cell_count: window.cells.len() as u64,
        },
        cells: window.cells.clone(),
        reference: Cycle4M3ReportReferenceWireV1 {
            decision_weighted_mean_standard_deviation: RealV1::from_f64_v1(reference_dispersion),
            dispersion_allowance: RealV1::from_f64_v1(gate.dispersion_allowance),
        },
        statistics,
        verdict: if gate.verdict_pass {
            CYCLE4_M3_VERDICT_PASS_V1
        } else {
            CYCLE4_M3_VERDICT_FAIL_V1
        }
        .to_owned(),
        failures: gate
            .failures
            .iter()
            .map(|failure| (*failure).to_owned())
            .collect(),
    };
    let bytes = to_canonical_json_bytes_v1(&report, CanonicalJsonNullPolicyV1::Forbid).map_err(
        |error| Cycle4M3AuditErrorV1::new("cycle4_m3_audit_v1_canonical_json", error.to_string()),
    )?;
    Ok((bytes, gate.verdict_pass))
}

/// Decodes and structurally validates an audit report (the routing
/// selector's input).
pub fn decode_cycle4_m3_audit_report_v1(bytes: &[u8]) -> Result<Cycle4M3AuditReportV1> {
    let schema = decode_cycle4_m3_schema_identity_v1(bytes, "cycle4_m3_audit_v1_report_schema")?;
    if schema == CYCLE4_M3_AUDIT_SCHEMA_LEGACY_V1 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_report_schema_v1_unsupported",
            format!(
                "schema {schema} names the obsolete report layout without the required reference block; expected {}",
                CYCLE4_M3_AUDIT_SCHEMA_V2
            ),
        ));
    }
    if schema != CYCLE4_M3_AUDIT_SCHEMA_V2 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_report_schema",
            format!("unexpected schema {schema}"),
        ));
    }
    let report: Cycle4M3AuditReportV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid).map_err(
            |error| {
                Cycle4M3AuditErrorV1::new("cycle4_m3_audit_v1_canonical_json", error.to_string())
            },
        )?;
    debug_assert_eq!(report.schema, CYCLE4_M3_AUDIT_SCHEMA_V2);
    if Cycle4M3ResidualModeV1::from_wire_v1(&report.residual_mode)?
        != Cycle4M3ResidualModeV1::Centered
    {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_audit_mode",
            "an eligibility report must be the CENTERED residual",
        ));
    }
    let pass = report.verdict == CYCLE4_M3_VERDICT_PASS_V1;
    if !pass && report.verdict != CYCLE4_M3_VERDICT_FAIL_V1 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_report_verdict",
            format!("unknown verdict {}", report.verdict),
        ));
    }
    if pass != report.failures.is_empty() {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_report_verdict",
            "the verdict and the failure list disagree",
        ));
    }
    // The thresholds a report was produced under must be this build's
    // ratified constants; a report from a differently-parameterized build is
    // not an input to this selector.
    if report.thresholds.qualifying_min_decisions != CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1
        || report.thresholds.coverage_floor_percent != CYCLE4_M3_COVERAGE_FLOOR_PERCENT_V1
        || report.thresholds.centering_max_abs_mean.to_f64_v1()?
            != CYCLE4_M3_CENTERING_MAX_ABS_MEAN_V1
        || report.thresholds.dispersion_ratio_max.to_f64_v1()? != CYCLE4_M3_DISPERSION_RATIO_MAX_V1
    {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_report_thresholds",
            "the report was produced under thresholds this build does not ratify",
        ));
    }
    if report.window.update_count != CYCLE4_M3_WINDOW_UPDATES_V1 {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_report_window",
            format!(
                "the report spans {} updates, not the pinned {}",
                report.window.update_count, CYCLE4_M3_WINDOW_UPDATES_V1
            ),
        ));
    }
    for cell in &report.cells {
        cell.mean_residual.to_f64_v1()?;
        cell.sample_standard_deviation.to_f64_v1()?;
        if cell.qualifies != (cell.decision_count >= CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1) {
            return Err(Cycle4M3AuditErrorV1::new(
                "cycle4_m3_audit_v1_report_cell_qualification",
                "a cell's declared qualification disagrees with its decision count",
            ));
        }
    }
    // Re-run the gate over the report's own cell table and require the same
    // verdict: a report whose verdict does not follow from its own numbers is
    // refused rather than believed.
    let reference_dispersion = report
        .reference
        .decision_weighted_mean_standard_deviation
        .to_f64_v1()?;
    report.reference.dispersion_allowance.to_f64_v1()?;
    // The totals are derived, never read: `window_decision_count` is the
    // coverage clause's DENOMINATOR, so an edited one would move the floor
    // without touching a single cell.
    let qualifying: Vec<&Cycle4M3CellV1> =
        report.cells.iter().filter(|cell| cell.qualifies).collect();
    let recomputed_totals = recompute_totals_v1(&report.cells, &qualifying)?;
    if recomputed_totals != report.totals {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_report_totals_mismatch",
            "the report totals do not follow from its own cell table",
        ));
    }
    let recomputed = evaluate_cycle4_m3_gate_v1(
        &report.cells,
        recomputed_totals.window_decision_count,
        reference_dispersion,
    );
    if recomputed.verdict_pass != pass {
        return Err(Cycle4M3AuditErrorV1::new(
            "cycle4_m3_audit_v1_report_verdict",
            "the report's verdict does not follow from its own cell table",
        ));
    }
    Ok(report)
}

/// Builds a window from an already-decided cell table, for tests in this
/// module and in `native_cycle4_routing_v1` that need a report without a
/// Store on disk. The digests are placeholders; nothing under test reads
/// them for meaning.
#[cfg(test)]
pub(crate) fn test_support_window_v1(
    residual_mode: Cycle4M3ResidualModeV1,
    cells: Vec<Cycle4M3CellV1>,
    decision_count: u64,
    run_sha256: String,
    tip_checkpoint_manifest_sha256: String,
) -> Cycle4M3WindowV1 {
    let placeholder = "ab".repeat(32);
    let centered = residual_mode == Cycle4M3ResidualModeV1::Centered;
    Cycle4M3WindowV1 {
        residual_mode,
        run_sha256,
        first_update_index: 1_537,
        last_update_index: 2_048,
        tip_update_evidence_sha256: placeholder.clone(),
        tip_checkpoint_manifest_sha256,
        evidence_chain_sha256: placeholder.clone(),
        sidecar_chain_sha256: centered.then(|| placeholder.clone()),
        prewindow_sidecar_chain_sha256: centered.then_some(placeholder),
        window_sidecars: Vec::new(),
        decision_count,
        cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_v1(tag: u8) -> String {
        format!("{tag:02x}").repeat(32)
    }

    fn cell_v1(tag: u8, role: &str, decisions: u64, mean: f64, sd: f64) -> Cycle4M3CellV1 {
        Cycle4M3CellV1 {
            opponent_checkpoint_manifest_sha256: identity_v1(tag),
            role: role.to_owned(),
            decision_count: decisions,
            episode_count: decisions / 30,
            mean_residual: RealV1::from_f64_v1(mean),
            sample_standard_deviation: RealV1::from_f64_v1(sd),
            qualifies: decisions >= CYCLE4_M3_QUALIFYING_MIN_DECISIONS_V1,
        }
    }

    #[test]
    fn real_round_trips_and_rejects_malformed_bits_v1() {
        let real = RealV1::from_f64_v1(-0.008_15);
        assert_eq!(real.to_f64_v1().expect("decode"), -0.008_15);
        assert_eq!(real.text, "-0.00815");
        // Non-finite, uppercase hex, and short input all fail closed.
        assert!(parse_f64_bits_hex_v1("7ff0000000000000").is_err());
        assert!(parse_f64_bits_hex_v1("00FF00FF00FF00FF").is_err());
        assert!(parse_f64_bits_hex_v1("0011").is_err());
    }

    /// Section A clause 1: "FAIL if no cell qualifies".
    #[test]
    fn fails_when_no_cell_qualifies_v1() {
        let cells = vec![
            cell_v1(1, "p0", 999, 0.0, 0.5),
            cell_v1(2, "p1", 400, 0.0, 0.5),
        ];
        let gate = evaluate_cycle4_m3_gate_v1(&cells, 1_399, 1.0);
        assert!(!gate.verdict_pass);
        assert_eq!(gate.failures, vec!["no_cell_qualifies"]);
        assert_eq!(gate.qualifying_cell_count, 0);
    }

    /// Section A clause 1: the 80% coverage floor. 1,200 qualifying of
    /// 2,000 decisions is 60%, so the sparse problem cells cannot vanish.
    #[test]
    fn fails_when_qualifying_cells_cover_too_little_v1() {
        let cells = vec![
            cell_v1(1, "p0", 1_200, 0.0, 0.5),
            cell_v1(2, "p1", 800, 0.9, 0.5),
        ];
        let gate = evaluate_cycle4_m3_gate_v1(&cells, 2_000, 1.0);
        assert!(!gate.verdict_pass);
        assert!(gate.failures.contains(&"coverage_below_floor"));
        assert_eq!(gate.qualifying_decision_count, 1_200);
    }

    /// Exactly 80% coverage passes the floor: the amendment fails BELOW 80%.
    #[test]
    fn coverage_at_exactly_the_floor_passes_v1() {
        let cells = vec![
            cell_v1(1, "p0", 1_600, 0.0, 0.5),
            cell_v1(2, "p1", 400, 0.9, 0.5),
        ];
        let gate = evaluate_cycle4_m3_gate_v1(&cells, 2_000, 1.0);
        assert!(!gate.failures.contains(&"coverage_below_floor"));
        assert!(gate.verdict_pass, "{:?}", gate.failures);
    }

    /// Section A centering statistic, both sides of 0.015.
    #[test]
    fn centering_threshold_holds_on_both_sides_v1() {
        let inside = vec![
            cell_v1(1, "p0", 5_000, 0.015, 0.5),
            cell_v1(2, "p1", 5_000, -0.014, 0.5),
        ];
        let gate = evaluate_cycle4_m3_gate_v1(&inside, 10_000, 1.0);
        assert!(gate.verdict_pass, "{:?}", gate.failures);
        assert_eq!(gate.max_abs_mean, Some(0.015));

        let outside = vec![
            cell_v1(1, "p0", 5_000, 0.0, 0.5),
            cell_v1(2, "p1", 5_000, -0.016, 0.5),
        ];
        let gate = evaluate_cycle4_m3_gate_v1(&outside, 10_000, 1.0);
        assert!(!gate.verdict_pass);
        assert_eq!(gate.failures, vec!["centering_above_threshold"]);
        assert_eq!(
            gate.max_abs_mean_cell,
            Some((identity_v1(2), "p1".to_owned()))
        );
    }

    /// Section A dispersion statistic, both sides of 1.10 x reference, with
    /// the decision weighting actually exercised (the two cells carry
    /// different weights, so an unweighted mean would give a different
    /// answer).
    #[test]
    fn dispersion_allowance_holds_on_both_sides_v1() {
        // Weighted mean sd = (3000*1.0 + 1000*2.0) / 4000 = 1.25.
        let cells = vec![
            cell_v1(1, "p0", 3_000, 0.0, 1.0),
            cell_v1(2, "p1", 1_000, 0.0, 2.0),
        ];
        let unweighted_mean = 1.5_f64;
        assert!(
            (decision_weighted_mean_standard_deviation_v1(&cells.iter().collect::<Vec<_>>())
                - 1.25)
                .abs()
                < 1e-12
        );
        assert!((unweighted_mean - 1.25).abs() > 1e-12);

        // Reference 1.25 / 1.10 leaves an allowance of exactly 1.25.
        let gate = evaluate_cycle4_m3_gate_v1(&cells, 4_000, 1.25 / 1.10);
        assert!(gate.verdict_pass, "{:?}", gate.failures);

        let gate = evaluate_cycle4_m3_gate_v1(&cells, 4_000, 1.0);
        assert!(!gate.verdict_pass);
        assert_eq!(gate.failures, vec!["dispersion_above_allowance"]);
        assert!((gate.dispersion_allowance - 1.1).abs() < 1e-12);
    }

    /// Every applicable clause is reported, not only the first.
    #[test]
    fn a_report_names_every_failing_clause_v1() {
        let cells = vec![
            cell_v1(1, "p0", 1_000, 0.9, 4.0),
            cell_v1(2, "p1", 900, 0.0, 0.1),
        ];
        let gate = evaluate_cycle4_m3_gate_v1(&cells, 10_000, 0.5);
        assert!(!gate.verdict_pass);
        assert_eq!(
            gate.failures,
            vec![
                "coverage_below_floor",
                "centering_above_threshold",
                "dispersion_above_allowance"
            ]
        );
    }

    #[test]
    fn welford_matches_the_textbook_sample_statistics_v1() {
        let mut accumulator = CellAccumulatorV1::default();
        let values = [1.0_f64, -1.0, 1.0, -1.0, 0.5];
        for value in values {
            accumulator.observe_v1(value);
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance =
            values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / (values.len() - 1) as f64;
        assert!((accumulator.mean - mean).abs() < 1e-12);
        assert!((accumulator.sample_standard_deviation_v1() - variance.sqrt()).abs() < 1e-12);
        assert_eq!(accumulator.count, 5);

        let single = {
            let mut accumulator = CellAccumulatorV1::default();
            accumulator.observe_v1(0.25);
            accumulator
        };
        assert_eq!(single.sample_standard_deviation_v1(), 0.0);
    }

    #[test]
    fn reference_document_round_trips_and_refuses_a_centered_window_v1() {
        let window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Raw,
            vec![
                cell_v1(1, "p0", 6_000, -0.008, 0.90),
                cell_v1(2, "p1", 4_000, 0.009, 0.94),
            ],
            10_000,
            identity_v1(7),
            identity_v1(8),
        );
        let bytes = build_cycle4_m3_reference_document_v1(&window, identity_v1(0x0a))
            .expect("reference document");
        let decoded = decode_cycle4_m3_reference_document_v1(&bytes).expect("decode");
        assert_eq!(decoded.schema, CYCLE4_M3_REFERENCE_SCHEMA_V2);
        assert_eq!(decoded.window.update_count, 512);
        assert_eq!(decoded.audit_note_sha256, identity_v1(0x0a));
        assert_eq!(decoded.tip_checkpoint_manifest_sha256, identity_v1(8));
        // (6000*0.90 + 4000*0.94) / 10000 = 0.916.
        let dispersion = decoded
            .decision_weighted_mean_standard_deviation
            .to_f64_v1()
            .expect("dispersion");
        assert!((dispersion - 0.916).abs() < 1e-12);

        let centered = Cycle4M3WindowV1 {
            residual_mode: Cycle4M3ResidualModeV1::Centered,
            ..window
        };
        assert_eq!(
            build_cycle4_m3_reference_document_v1(&centered, identity_v1(0x0a))
                .expect_err("centered window must be refused")
                .code(),
            "cycle4_m3_audit_v1_reference_mode"
        );
    }

    #[test]
    fn the_legacy_v1_reference_identity_is_refused_before_layout_decode_v1() {
        let bytes = build_cycle4_m3_reference_document_v1(
            &test_support_window_v1(
                Cycle4M3ResidualModeV1::Raw,
                vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
                10_000,
                identity_v1(7),
                identity_v1(8),
            ),
            identity_v1(0x0a),
        )
        .expect("reference document");
        let mut legacy: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON value");
        legacy["schema"] =
            serde_json::Value::String(CYCLE4_M3_REFERENCE_SCHEMA_LEGACY_V1.to_owned());
        legacy
            .as_object_mut()
            .expect("reference object")
            .remove("tip_checkpoint_manifest_sha256");
        let legacy_bytes = to_canonical_json_bytes_v1(&legacy, CanonicalJsonNullPolicyV1::Forbid)
            .expect("legacy canonical bytes");
        let error = decode_cycle4_m3_reference_document_v1(&legacy_bytes)
            .expect_err("the legacy reference identity must be refused");
        assert_eq!(
            error.code(),
            "cycle4_m3_audit_v1_reference_schema_v1_unsupported"
        );
        assert!(error.detail().contains(CYCLE4_M3_REFERENCE_SCHEMA_V2));
    }

    #[test]
    fn the_reference_builder_refuses_a_1024_update_window_v1() {
        let mut window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Raw,
            vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
            10_000,
            identity_v1(7),
            identity_v1(8),
        );
        window.set_window_bounds_for_test_v1(1_025, 2_048);
        assert_eq!(
            build_cycle4_m3_reference_document_v1(&window, identity_v1(0x0a))
                .expect_err("the producer must refuse a 1024-update reference window")
                .code(),
            "cycle4_m3_audit_v1_reference_window"
        );
    }

    #[test]
    fn the_reference_builder_refuses_a_1536_tip_v1() {
        let mut window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Raw,
            vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
            10_000,
            identity_v1(7),
            identity_v1(8),
        );
        window.set_window_bounds_for_test_v1(1_025, 1_536);
        assert_eq!(
            build_cycle4_m3_reference_document_v1(&window, identity_v1(0x0a))
                .expect_err("the producer must refuse a reference ending at update 1536")
                .code(),
            "cycle4_m3_audit_v1_reference_window"
        );
    }

    #[test]
    fn an_internally_inconsistent_reference_window_count_is_refused_v1() {
        let window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Raw,
            vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
            10_000,
            identity_v1(7),
            identity_v1(8),
        );
        let bytes = build_cycle4_m3_reference_document_v1(&window, identity_v1(0x0a))
            .expect("reference document");
        let mut document = decode_cycle4_m3_reference_document_v1(&bytes).expect("decode");
        document.window.update_count = 511;
        let encoded = to_canonical_json_bytes_v1(&document, CanonicalJsonNullPolicyV1::Forbid)
            .expect("re-encode");
        assert_eq!(
            decode_cycle4_m3_reference_document_v1(&encoded)
                .expect_err("an inconsistent reference window count must be refused")
                .code(),
            "cycle4_m3_audit_v1_reference_window"
        );
    }

    /// The reference statistic and the totals that decide which cells enter
    /// it are recomputed on decode, so an edited value is refused rather than
    /// silently loosening the whole dispersion clause.
    #[test]
    fn an_edited_reference_statistic_is_refused_v1() {
        let window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Raw,
            vec![
                cell_v1(1, "p0", 6_000, -0.008, 0.90),
                cell_v1(2, "p1", 4_000, 0.009, 0.94),
            ],
            10_000,
            identity_v1(7),
            identity_v1(8),
        );
        let bytes = build_cycle4_m3_reference_document_v1(&window, identity_v1(0x0a))
            .expect("reference document");
        let mut document = decode_cycle4_m3_reference_document_v1(&bytes).expect("decode");

        // A reference inflated from 0.916 to 2.0 would let any dispersion
        // through; it does not follow from the cell table and is refused.
        let mut edited = document.clone();
        edited.decision_weighted_mean_standard_deviation = RealV1::from_f64_v1(2.0);
        let encoded = to_canonical_json_bytes_v1(&edited, CanonicalJsonNullPolicyV1::Forbid)
            .expect("re-encode");
        assert_eq!(
            decode_cycle4_m3_reference_document_v1(&encoded)
                .expect_err("an edited statistic must be refused")
                .code(),
            "cycle4_m3_audit_v1_reference_statistic_mismatch"
        );

        // So is an edited total, which would change which cells the
        // statistic is taken over.
        let mut edited = document.clone();
        edited.totals.qualifying_decision_count += 1;
        let encoded = to_canonical_json_bytes_v1(&edited, CanonicalJsonNullPolicyV1::Forbid)
            .expect("re-encode");
        assert_eq!(
            decode_cycle4_m3_reference_document_v1(&encoded)
                .expect_err("edited totals must be refused")
                .code(),
            "cycle4_m3_audit_v1_reference_totals_mismatch"
        );

        // And so is an edited WINDOW DECISION COUNT, which is the coverage
        // clause's denominator: it is the checked sum of every cell's
        // decision count, never the declared value.
        document.totals.window_decision_count += 1_000;
        let encoded = to_canonical_json_bytes_v1(&document, CanonicalJsonNullPolicyV1::Forbid)
            .expect("re-encode");
        assert_eq!(
            decode_cycle4_m3_reference_document_v1(&encoded)
                .expect_err("an edited window decision count must be refused")
                .code(),
            "cycle4_m3_audit_v1_reference_totals_mismatch"
        );
    }

    /// The same denominator guard on the audit report, where an inflated
    /// `window_decision_count` would drop the coverage ratio below the floor
    /// (or lift it above one) without touching a single cell.
    #[test]
    fn an_edited_report_window_decision_count_is_refused_v1() {
        let reference_bytes = build_cycle4_m3_reference_document_v1(
            &test_support_window_v1(
                Cycle4M3ResidualModeV1::Raw,
                vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
                10_000,
                identity_v1(7),
                identity_v1(8),
            ),
            identity_v1(0x0a),
        )
        .expect("reference");
        let reference =
            decode_cycle4_m3_reference_document_v1(&reference_bytes).expect("decode reference");
        let window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Centered,
            vec![
                cell_v1(1, "p0", 8_000, 0.001, 1.0),
                cell_v1(2, "p1", 2_000, 0.001, 1.0),
            ],
            10_000,
            identity_v1(3),
            identity_v1(4),
        );
        let (bytes, pass) =
            build_cycle4_m3_audit_report_v1("static-rb", &window, &reference, &identity_v1(0x0d))
                .expect("report");
        assert!(pass);

        let mut report = decode_cycle4_m3_audit_report_v1(&bytes).expect("decode");
        // 10,000 of a declared 40,000 is 25% coverage: an unchecked
        // denominator would flip this report to FAIL on numbers no cell
        // supports. The decoder derives it instead.
        report.totals.window_decision_count = 40_000;
        let encoded = to_canonical_json_bytes_v1(&report, CanonicalJsonNullPolicyV1::Forbid)
            .expect("re-encode");
        assert_eq!(
            decode_cycle4_m3_audit_report_v1(&encoded)
                .expect_err("an edited denominator must be refused")
                .code(),
            "cycle4_m3_audit_v1_report_totals_mismatch"
        );
    }

    #[test]
    fn audit_report_round_trips_and_carries_the_verdict_v1() {
        // Uses the shared test-support constructor, which the routing
        // selector's own tests also build their fixtures from.
        let reference_bytes = build_cycle4_m3_reference_document_v1(
            &test_support_window_v1(
                Cycle4M3ResidualModeV1::Raw,
                vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
                10_000,
                identity_v1(7),
                identity_v1(8),
            ),
            identity_v1(0x0a),
        )
        .expect("reference");
        let reference =
            decode_cycle4_m3_reference_document_v1(&reference_bytes).expect("decode reference");

        let window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Centered,
            vec![cell_v1(1, "p0", 10_000, 0.001, 1.05)],
            10_000,
            identity_v1(3),
            identity_v1(4),
        );
        let (bytes, pass) = build_cycle4_m3_audit_report_v1(
            "treatment-rb",
            &window,
            &reference,
            &identity_v1(0x0d),
        )
        .expect("report");
        assert!(pass);
        let decoded = decode_cycle4_m3_audit_report_v1(&bytes).expect("decode report");
        assert_eq!(decoded.schema, CYCLE4_M3_AUDIT_SCHEMA_V2);
        assert_eq!(decoded.verdict, CYCLE4_M3_VERDICT_PASS_V1);
        assert_eq!(decoded.arm_kind, "treatment-rb");
        assert_eq!(decoded.inputs.reference_document_sha256, identity_v1(0x0d));
        assert!(decoded.failures.is_empty());

        // 1.20 > 1.10 * 1.0, so the same report shape fails the dispersion
        // clause and says so.
        let failing = Cycle4M3WindowV1 {
            cells: vec![cell_v1(1, "p0", 10_000, 0.001, 1.20)],
            ..window
        };
        let (bytes, pass) =
            build_cycle4_m3_audit_report_v1("static-rb", &failing, &reference, &identity_v1(0x0d))
                .expect("report");
        assert!(!pass);
        let decoded = decode_cycle4_m3_audit_report_v1(&bytes).expect("decode report");
        assert_eq!(decoded.verdict, CYCLE4_M3_VERDICT_FAIL_V1);
        assert_eq!(decoded.failures, vec!["dispersion_above_allowance"]);
    }

    #[test]
    fn the_legacy_v1_report_identity_is_refused_before_layout_decode_v1() {
        let reference_bytes = build_cycle4_m3_reference_document_v1(
            &test_support_window_v1(
                Cycle4M3ResidualModeV1::Raw,
                vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
                10_000,
                identity_v1(7),
                identity_v1(8),
            ),
            identity_v1(0x0a),
        )
        .expect("reference");
        let reference =
            decode_cycle4_m3_reference_document_v1(&reference_bytes).expect("decode reference");
        let window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Centered,
            vec![cell_v1(1, "p0", 10_000, 0.001, 1.05)],
            10_000,
            identity_v1(3),
            identity_v1(4),
        );
        let (bytes, _) =
            build_cycle4_m3_audit_report_v1("static-rb", &window, &reference, &identity_v1(0x0d))
                .expect("report");
        let mut legacy: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON value");
        legacy["schema"] = serde_json::Value::String(CYCLE4_M3_AUDIT_SCHEMA_LEGACY_V1.to_owned());
        let report = legacy.as_object_mut().expect("report object");
        let reference = report.remove("reference").expect("reference block");
        let statistics = report
            .get_mut("statistics")
            .and_then(serde_json::Value::as_object_mut)
            .expect("statistics block");
        statistics.insert(
            "reference_decision_weighted_mean_standard_deviation".to_owned(),
            reference["decision_weighted_mean_standard_deviation"].clone(),
        );
        statistics.insert(
            "dispersion_allowance".to_owned(),
            reference["dispersion_allowance"].clone(),
        );
        let legacy_bytes = to_canonical_json_bytes_v1(&legacy, CanonicalJsonNullPolicyV1::Forbid)
            .expect("legacy canonical bytes");
        let error = decode_cycle4_m3_audit_report_v1(&legacy_bytes)
            .expect_err("the legacy report identity must be refused");
        assert_eq!(
            error.code(),
            "cycle4_m3_audit_v1_report_schema_v1_unsupported"
        );
        assert!(error.detail().contains(CYCLE4_M3_AUDIT_SCHEMA_V2));
    }

    #[test]
    fn a_no_cell_qualifies_report_keeps_its_reference_v1() {
        let reference_bytes = build_cycle4_m3_reference_document_v1(
            &test_support_window_v1(
                Cycle4M3ResidualModeV1::Raw,
                vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
                10_000,
                identity_v1(7),
                identity_v1(8),
            ),
            identity_v1(0x0a),
        )
        .expect("reference");
        let reference =
            decode_cycle4_m3_reference_document_v1(&reference_bytes).expect("decode reference");
        let window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Centered,
            vec![cell_v1(1, "p0", 999, 0.001, 1.05)],
            999,
            identity_v1(3),
            identity_v1(4),
        );
        let (bytes, pass) =
            build_cycle4_m3_audit_report_v1("static-rb", &window, &reference, &identity_v1(0x0d))
                .expect("FAIL report");
        assert!(!pass);
        let report = decode_cycle4_m3_audit_report_v1(&bytes).expect("decode FAIL report");
        assert_eq!(report.verdict, CYCLE4_M3_VERDICT_FAIL_V1);
        assert_eq!(report.failures, vec!["no_cell_qualifies"]);
        assert!(report.statistics.is_none());
        assert_eq!(
            report
                .reference
                .decision_weighted_mean_standard_deviation
                .to_f64_v1()
                .expect("reference dispersion"),
            1.0
        );
        assert_eq!(
            report
                .reference
                .dispersion_allowance
                .to_f64_v1()
                .expect("allowance"),
            1.1
        );
    }

    // -----------------------------------------------------------------
    // Synthetic evidence on disk: the Store/chain reading path end to end.
    // -----------------------------------------------------------------

    use crate::native_training_store_update_group_v4::build_update_baseline_record_from_episodes_v4;

    struct TestStoreV1 {
        root: std::path::PathBuf,
    }

    impl TestStoreV1 {
        fn new_v1(label: &str) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "mtg-kernel-cycle4-m3-{}-{label}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(root.join(SEGMENT_DIRECTORY_V1)).expect("segments dir");
            fs::create_dir_all(root.join(CHECKPOINT_DIRECTORY_V1)).expect("checkpoints dir");
            fs::create_dir_all(root.join("chain")).expect("chain dir");
            Self { root }
        }

        fn store_root_v1(&self) -> PathBuf {
            self.root.clone()
        }

        fn chain_dir_v1(&self) -> PathBuf {
            self.root.join("chain")
        }
    }

    impl Drop for TestStoreV1 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// One synthetic episode: a cell, a learner return, and the value bits of
    /// each of its learner physical terms.
    #[derive(Clone)]
    struct SyntheticEpisodeV1 {
        identity: String,
        role: BaselineRoleV4,
        learner_return: i8,
        values: Vec<f32>,
        /// `Some((authority sha256, tier))` emits a search-occupant episode:
        /// no checkpoint manifest, the two search fields instead.
        search_occupant: Option<(String, String)>,
    }

    fn synthetic_episode_v1(
        tag: u8,
        role: BaselineRoleV4,
        learner_return: i8,
        values: &[f32],
    ) -> SyntheticEpisodeV1 {
        SyntheticEpisodeV1 {
            identity: identity_v1(tag),
            role,
            learner_return,
            values: values.to_vec(),
            search_occupant: None,
        }
    }

    const SYNTHETIC_JOINT_LOG_PROBABILITY_V1: f32 = -0.5;

    fn synthetic_evidence_json_v1(
        run_sha256: &str,
        update_index: u64,
        episodes: &[SyntheticEpisodeV1],
    ) -> serde_json::Value {
        let mut terms = Vec::new();
        let mut episode_rows = Vec::new();
        for (ordinal, episode) in episodes.iter().enumerate() {
            for value in &episode.values {
                terms.push(serde_json::json!({
                    "joint_log_probability_f32_bits":
                        format!("{:08x}", SYNTHETIC_JOINT_LOG_PROBABILITY_V1.to_bits()),
                    "value_f32_bits": format!("{:08x}", value.to_bits()),
                    "terminal_return_i8": episode.learner_return,
                    "substep_count": 1,
                }));
            }
            let mut row = serde_json::json!({
                "schema": EPISODE_SCHEMA_V1,
                "episode_index": ordinal as u64,
                "environment_seed_u64_hex": "0000000000000001",
                "deck_ids": ["deck-a", "deck-b"],
                "deck_hashes_u64_hex": ["0000000000000002", "0000000000000003"],
                "learner_seat": episode.role.wire_v4(),
                "learner_return": episode.learner_return,
                "terminal_outcome": "draw",
                "winner": serde_json::Value::Null,
                "terminal_classification": "Natural",
                "terminal_code": "NaturalGameOver",
                "policy_step_count": episode.values.len() as u64,
                "physical_decision_count": episode.values.len() as u64,
                "learner_policy_step_count": episode.values.len() as u64,
                "opponent_policy_step_count": 0,
                "learner_physical_decision_count": episode.values.len() as u64,
                "opponent_physical_decision_count": 0,
                "trajectory_sha256": identity_v1(0x77),
                "opponent_population_slot": 0,
                "opponent_occupant_class": "policy",
                "opponent_run_sha256": identity_v1(0x78),
                "opponent_checkpoint_manifest_sha256": episode.identity,
            });
            if let Some((authority, tier)) = &episode.search_occupant {
                let object = row.as_object_mut().expect("episode row object");
                object.remove("opponent_checkpoint_manifest_sha256");
                object.remove("opponent_run_sha256");
                object.insert(
                    "opponent_occupant_class".to_owned(),
                    serde_json::Value::String("kernel-native-search".to_owned()),
                );
                object.insert(
                    "opponent_search_authority_sha256".to_owned(),
                    serde_json::Value::String(authority.clone()),
                );
                object.insert(
                    "opponent_search_tier".to_owned(),
                    serde_json::Value::String(tier.clone()),
                );
            }
            episode_rows.push(row);
        }
        serde_json::json!({
            "schema": UPDATE_EVIDENCE_SCHEMA_V1,
            "run_sha256": run_sha256,
            "update_index": update_index,
            "learner_physical_decision_count": terms.len() as u64,
            "physical_terms": terms,
            "episodes": episode_rows,
        })
    }

    /// One episode's view header: the cell role, the learner return, and the
    /// opponent identity. Named so the fixture's return type stays readable.
    type SyntheticHeaderV1 = (BaselineRoleV4, i8, String);

    fn synthetic_views_v1(
        episodes: &[SyntheticEpisodeV1],
    ) -> (Vec<Vec<UpdateBaselineTermViewV4>>, Vec<SyntheticHeaderV1>) {
        let mut terms = Vec::new();
        let mut headers = Vec::new();
        for episode in episodes {
            terms.push(
                episode
                    .values
                    .iter()
                    .map(|value| UpdateBaselineTermViewV4 {
                        joint_log_probability_f32_bits: SYNTHETIC_JOINT_LOG_PROBABILITY_V1
                            .to_bits(),
                        value_f32_bits: value.to_bits(),
                        terminal_return_i8: episode.learner_return,
                    })
                    .collect::<Vec<_>>(),
            );
            headers.push((
                episode.role,
                episode.learner_return,
                episode.identity.clone(),
            ));
        }
        (terms, headers)
    }

    /// Writes one continuation file holding `updates` consecutive updates
    /// starting at index 1, plus (when `chain` is true) the matching v4
    /// sidecar chain from the empty genesis state.
    fn write_synthetic_store_v1(
        store: &TestStoreV1,
        run_sha256: &str,
        updates: &[Vec<SyntheticEpisodeV1>],
        chain: bool,
    ) {
        let mut groups = Vec::new();
        let mut prior = NativeBaselineStateV4::empty_v4();
        // The fixture builds a REAL evidence-digest chain: each update's
        // digest is computed over its own canonical evidence bytes with the
        // Store's own domain and atom order, and names its predecessor. The
        // audit recomputes exactly this, so a fixture with invented digests
        // would not exercise the check at all.
        let run_digest = parse_lower_hex_raw32_v1(run_sha256).expect("run sha256");
        let mut previous_digest: Option<[u8; 32]> = None;
        for (ordinal, episodes) in updates.iter().enumerate() {
            let update_index = ordinal as u64 + 1;
            let evidence = synthetic_evidence_json_v1(run_sha256, update_index, episodes);
            let evidence_cj = to_canonical_json_bytes_v1(&evidence, M3_EVIDENCE_NULL_POLICY_V1)
                .expect("canonical evidence bytes");
            let evidence_digest = recompute_update_evidence_sha256_v1(
                run_digest,
                update_index,
                previous_digest,
                &evidence_cj,
            )
            .expect("evidence digest");
            groups.push(serde_json::json!({
                "update_index": update_index,
                "update_evidence_sha256": lower_hex_raw32_v1(evidence_digest),
                "previous_update_evidence_sha256": previous_digest
                    .map_or(serde_json::Value::Null, |digest| {
                        serde_json::Value::String(lower_hex_raw32_v1(digest))
                    }),
                "evidence": evidence,
            }));
            previous_digest = Some(evidence_digest);
            if !chain {
                continue;
            }
            let (terms, headers) = synthetic_views_v1(episodes);
            let views = headers
                .iter()
                .zip(terms.iter())
                .map(
                    |((role, learner_return, identity), slice)| UpdateBaselineEpisodeViewV4 {
                        learner_seat: *role,
                        learner_return: *learner_return,
                        opponent_checkpoint_manifest_sha256: identity.as_str(),
                        terms: slice.as_slice(),
                    },
                )
                .collect::<Vec<_>>();
            // The declared policy sum has to be what the v4 recompute
            // produces, in the same batch order and the same f32 arithmetic.
            let mut policy_sum = 0.0_f32;
            for view in &views {
                let key = BaselineCellKeyV4::new_v4(
                    view.opponent_checkpoint_manifest_sha256,
                    view.learner_seat,
                )
                .expect("cell key");
                let c_t = prior.c_for_cell_v4(&key);
                for term in view.terms {
                    let target = f32::from(term.terminal_return_i8);
                    let value = f32::from_bits(term.value_f32_bits);
                    policy_sum += (-f32::from_bits(term.joint_log_probability_f32_bits))
                        * ((target - value) - c_t);
                }
            }
            let record = build_update_baseline_record_from_episodes_v4(
                &views,
                &prior,
                update_index,
                evidence_digest,
                policy_sum.to_bits(),
            )
            .expect("sidecar record");
            fs::write(
                store
                    .chain_dir_v1()
                    .join(format!("baseline-update-{update_index:08}.record.json")),
                record.canonical_bytes(),
            )
            .expect("write sidecar");
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
            prior = prior.apply_update_v4(&observations).expect("advance");
        }
        let continuation = serde_json::json!({ "update_groups": groups });
        fs::write(
            store
                .store_root_v1()
                .join(SEGMENT_DIRECTORY_V1)
                .join("segment-00000004.continuation-00000000.json"),
            to_canonical_json_bytes_v1(&continuation, M3_CONTINUATION_NULL_POLICY_V1)
                .expect("canonical continuation bytes"),
        )
        .expect("write continuation");
        // The audit reads the tip checkpoint manifest to name the identity
        // the M2 probe reports for the same checkpoint, so the fixture
        // publishes one.
        let tip = updates.len() as u64;
        fs::write(
            store
                .store_root_v1()
                .join(CHECKPOINT_DIRECTORY_V1)
                .join(format!("update-{tip:08}.checkpoint.json")),
            format!("{{\"synthetic_checkpoint_for_update\":{tip}}}\n"),
        )
        .expect("write tip checkpoint");
    }

    /// Rewrites the fixture's continuation with `edit` applied to the parsed
    /// tree, re-canonicalizing so only the intended change is visible. The
    /// evidence digests are deliberately NOT recomputed: an edit is meant to
    /// be caught.
    fn edit_continuation_v1(store: &TestStoreV1, edit: impl FnOnce(&mut serde_json::Value)) {
        let path = store
            .store_root_v1()
            .join(SEGMENT_DIRECTORY_V1)
            .join("segment-00000004.continuation-00000000.json");
        let mut document: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read continuation")).expect("parse");
        edit(&mut document);
        fs::write(
            &path,
            to_canonical_json_bytes_v1(&document, M3_CONTINUATION_NULL_POLICY_V1)
                .expect("canonical continuation bytes"),
        )
        .expect("write continuation");
    }

    fn two_cell_update_v1(p0_values: &[f32], p1_values: &[f32]) -> Vec<SyntheticEpisodeV1> {
        vec![
            synthetic_episode_v1(1, BaselineRoleV4::P0, 1, p0_values),
            synthetic_episode_v1(2, BaselineRoleV4::P1, -1, p1_values),
        ]
    }

    #[test]
    fn a_1536_tip_synthetic_store_cannot_publish_a_reference_v1() {
        let store = TestStoreV1::new_v1("reference-tip-1536");
        let run_sha256 = identity_v1(0x31);
        let update = vec![synthetic_episode_v1(1, BaselineRoleV4::P0, 1, &[0.0, 0.0])];
        write_synthetic_store_v1(&store, &run_sha256, &vec![update; 1_536], false);

        let window = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: None,
            residual_mode: Cycle4M3ResidualModeV1::Raw,
            window_updates: CYCLE4_M3_WINDOW_UPDATES_V1,
        })
        .expect("the synthetic Store has a complete final 512-update window");
        assert_eq!(window.first_update_index(), 1_025);
        assert_eq!(window.last_update_index(), 1_536);
        assert_eq!(
            build_cycle4_m3_reference_document_v1(&window, identity_v1(0x0a))
                .expect_err("reference publication input must be refused before serialization")
                .code(),
            "cycle4_m3_audit_v1_reference_window"
        );
    }

    /// The RAW path end to end: cells, counts, means and sample standard
    /// deviations recovered from evidence on disk, with no chain at all.
    #[test]
    fn raw_window_recovers_cells_from_store_evidence_v1() {
        let store = TestStoreV1::new_v1("raw-window");
        let run_sha256 = identity_v1(0x5a);
        let updates = vec![
            two_cell_update_v1(&[0.2, 0.4], &[0.1]),
            two_cell_update_v1(&[0.6, 0.0], &[-0.3]),
        ];
        write_synthetic_store_v1(&store, &run_sha256, &updates, false);

        let window = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: None,
            residual_mode: Cycle4M3ResidualModeV1::Raw,
            window_updates: 2,
        })
        .expect("raw window");

        assert_eq!(window.run_sha256(), run_sha256);
        assert_eq!(window.first_update_index(), 1);
        assert_eq!(window.last_update_index(), 2);
        assert_eq!(window.decision_count(), 6);
        assert_eq!(window.cells().len(), 2);

        // Cell (identity 1, p0): learner_return +1, values 0.2/0.4/0.6/0.0
        // give raw residuals 0.8/0.6/0.4/1.0, mean 0.7.
        let p0 = window
            .cells()
            .iter()
            .find(|cell| cell.role == "p0")
            .expect("p0 cell");
        assert_eq!(p0.decision_count, 4);
        assert_eq!(p0.episode_count, 2);
        let mean = p0.mean_residual.to_f64_v1().expect("mean");
        assert!((mean - 0.7).abs() < 1e-6, "{mean}");

        // Cell (identity 2, p1): learner_return -1, values 0.1/-0.3 give
        // residuals -1.1/-0.7, mean -0.9, sample sd |diff| / sqrt(2) * ... =
        // sqrt(((0.2)^2 + (0.2)^2)/1) = 0.2828...
        let p1 = window
            .cells()
            .iter()
            .find(|cell| cell.role == "p1")
            .expect("p1 cell");
        assert_eq!(p1.decision_count, 2);
        let mean = p1.mean_residual.to_f64_v1().expect("mean");
        assert!((mean + 0.9).abs() < 1e-6, "{mean}");
        let sd = p1.sample_standard_deviation.to_f64_v1().expect("sd");
        assert!((sd - (0.08_f64).sqrt()).abs() < 1e-6, "{sd}");
        assert!(!p0.qualifies && !p1.qualifies);
    }

    /// The CENTERED path end to end: the sidecar chain is replayed from the
    /// empty genesis state, `validate_update_baseline_v4` proves each window
    /// update against its own evidence, and the recovered `c_t` shifts the
    /// residuals. Update 1 sees `c = 0`, so only update 2 is centered.
    #[test]
    fn centered_window_recovers_c_t_through_the_v4_validator_v1() {
        let store = TestStoreV1::new_v1("centered-window");
        let run_sha256 = identity_v1(0x5b);
        let updates = vec![
            two_cell_update_v1(&[0.2, 0.4], &[0.1]),
            two_cell_update_v1(&[0.6, 0.0], &[-0.3]),
        ];
        write_synthetic_store_v1(&store, &run_sha256, &updates, true);

        // The whole store as the window: both updates centered, update 1 at
        // c = 0.
        let full = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: Some(store.chain_dir_v1()),
            residual_mode: Cycle4M3ResidualModeV1::Centered,
            window_updates: 2,
        })
        .expect("centered window");
        assert_eq!(full.window_sidecars.len(), 2);
        assert!(full.sidecar_chain_sha256.is_some());
        assert_eq!(full.decision_count(), 6);

        // A one-update window: update 1 is replayed sidecar-only to reach the
        // window's opening state, and update 2's residuals are centered by
        // the `c_1` that replay produced. Update 1's p0 mean raw residual is
        // 0.7, so c_1(p0) = 0.05 * 0.7 = 0.035 and update 2's p0 residuals
        // 0.4/1.0 become 0.365/0.965 with mean 0.665.
        let tail = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: Some(store.chain_dir_v1()),
            residual_mode: Cycle4M3ResidualModeV1::Centered,
            window_updates: 1,
        })
        .expect("centered tail window");
        assert_eq!(tail.first_update_index(), 2);
        assert_eq!(tail.window_sidecars.len(), 1);
        let p0 = tail
            .cells()
            .iter()
            .find(|cell| cell.role == "p0")
            .expect("p0 cell");
        let mean = p0.mean_residual.to_f64_v1().expect("mean");
        assert!((mean - 0.665).abs() < 1e-5, "{mean}");

        // The RAW reading of the same tail window is 0.7: the centering is
        // real, not a relabeled copy.
        let raw_tail = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: None,
            residual_mode: Cycle4M3ResidualModeV1::Raw,
            window_updates: 1,
        })
        .expect("raw tail window");
        let raw_mean = raw_tail
            .cells()
            .iter()
            .find(|cell| cell.role == "p0")
            .expect("p0 cell")
            .mean_residual
            .to_f64_v1()
            .expect("mean");
        assert!((raw_mean - 0.7).abs() < 1e-6, "{raw_mean}");
    }

    /// Tampering with a value bit in the evidence moves the per-cell residual
    /// sum, which the v4 validator recomputes and compares against the
    /// sidecar's declared bits. This is the integrity argument the partial
    /// container mirrors rest on, exercised rather than asserted.
    #[test]
    fn tampered_evidence_fails_the_evidence_digest_v1() {
        let store = TestStoreV1::new_v1("tampered-evidence");
        let run_sha256 = identity_v1(0x5c);
        let updates = vec![two_cell_update_v1(&[0.25, 0.5], &[0.125])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, true);

        edit_continuation_v1(&store, |document| {
            document["update_groups"][0]["evidence"]["physical_terms"][0]["value_f32_bits"] =
                serde_json::Value::String(format!("{:08x}", 0.375_f32.to_bits()));
        });

        let error = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: Some(store.chain_dir_v1()),
            residual_mode: Cycle4M3ResidualModeV1::Centered,
            window_updates: 1,
        })
        .expect_err("tampered evidence must fail closed");
        assert_eq!(error.code(), "cycle4_m3_audit_v1_evidence_digest_mismatch");
    }

    /// THE case the sidecar cross-check alone cannot see. Two terms of one
    /// cell carry the same joint log-probability, so moving them in opposite
    /// directions by the same amount leaves the cell's residual SUM, its
    /// counts, the cell set and even the v4 policy sum bit-identical, while
    /// the cell's sample standard deviation changes. Every quantity the
    /// sidecar declares still reconciles; only the evidence digest moves.
    #[test]
    fn perturbed_evidence_preserving_the_sidecar_sums_fails_the_digest_v1() {
        let store = TestStoreV1::new_v1("perturbed-evidence");
        let run_sha256 = identity_v1(0x60);
        // Powers of two, so every f32 residual and its f64 widening is exact
        // and the two readings really do agree to the bit.
        let updates = vec![two_cell_update_v1(&[0.25, 0.5], &[0.125])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, true);

        // 0.25 -> 0.125 and 0.5 -> 0.625: the residual sum is 1.25 either
        // way, and the spread widens from 0.25 to 0.5.
        let clean = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: Some(store.chain_dir_v1()),
            residual_mode: Cycle4M3ResidualModeV1::Centered,
            window_updates: 1,
        })
        .expect("clean window");
        let clean_sd = clean
            .cells()
            .iter()
            .find(|cell| cell.role == "p0")
            .expect("p0 cell")
            .sample_standard_deviation
            .to_f64_v1()
            .expect("sd");

        edit_continuation_v1(&store, |document| {
            let terms = &mut document["update_groups"][0]["evidence"]["physical_terms"];
            terms[0]["value_f32_bits"] =
                serde_json::Value::String(format!("{:08x}", 0.125_f32.to_bits()));
            terms[1]["value_f32_bits"] =
                serde_json::Value::String(format!("{:08x}", 0.625_f32.to_bits()));
        });

        let error = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: Some(store.chain_dir_v1()),
            residual_mode: Cycle4M3ResidualModeV1::Centered,
            window_updates: 1,
        })
        .expect_err("a sum-preserving perturbation must still fail closed");
        assert_eq!(error.code(), "cycle4_m3_audit_v1_evidence_digest_mismatch");

        // And the perturbation really would have moved the statistic: run the
        // RAW reading of the edited store, which does not consult a sidecar,
        // to show the sample standard deviation the audit would otherwise
        // have reported.
        let perturbed = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: None,
            residual_mode: Cycle4M3ResidualModeV1::Raw,
            window_updates: 1,
        });
        assert_eq!(
            perturbed
                .expect_err("the raw reading is guarded by the same digest")
                .code(),
            "cycle4_m3_audit_v1_evidence_digest_mismatch"
        );
        assert!(
            (clean_sd - (0.03125_f64).sqrt()).abs() < 1e-12,
            "clean sd was {clean_sd}"
        );
    }

    /// A Store replaced BETWEEN the two filesystem passes is refused, even
    /// when the replacement is fully self-consistent: fresh evidence, a
    /// freshly recomputed digest chain, and matching sidecars. Pass one fixed
    /// the tip's digest; pass two must find the same one. Without the
    /// comparison the tip update was entirely free, since no later record
    /// links to it.
    #[test]
    fn evidence_replaced_between_passes_is_refused_v1() {
        let store = TestStoreV1::new_v1("between-passes");
        let run_sha256 = identity_v1(0x65);
        write_synthetic_store_v1(
            &store,
            &run_sha256,
            &[two_cell_update_v1(&[0.25, 0.5], &[0.125])],
            true,
        );

        let replacement_root = store.root.clone();
        let replacement_run = run_sha256.clone();
        let _fault = BetweenPassesFaultGuardV1::install_v1(move || {
            // A whole second Store, self-consistent end to end, written over
            // the first between the chain walk and the evidence read.
            let store = TestStoreV1 {
                root: replacement_root.clone(),
            };
            write_synthetic_store_v1(
                &store,
                &replacement_run,
                &[two_cell_update_v1(&[0.125, 0.625], &[0.125])],
                true,
            );
            std::mem::forget(store);
        });

        let error = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: Some(store.chain_dir_v1()),
            residual_mode: Cycle4M3ResidualModeV1::Centered,
            window_updates: 1,
        })
        .expect_err("a between-pass replacement must fail closed");
        assert_eq!(
            error.code(),
            "cycle4_m3_audit_v1_evidence_replaced_between_passes"
        );
    }

    /// The sidecar cross-check still guards the other direction: a sidecar
    /// whose declared residual sum does not follow from the evidence fails
    /// through `validate_update_baseline_v4`.
    #[test]
    fn a_tampered_sidecar_fails_the_v4_cross_check_v1() {
        let store = TestStoreV1::new_v1("tampered-sidecar");
        let run_sha256 = identity_v1(0x61);
        let updates = vec![two_cell_update_v1(&[0.25, 0.5], &[0.125])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, true);

        let path = store
            .chain_dir_v1()
            .join("baseline-update-00000001.record.json");
        let text = fs::read_to_string(&path).expect("read sidecar");
        let tampered = text.replace(
            &format!("{:016x}", 1.25_f64.to_bits()),
            &format!("{:016x}", 1.5_f64.to_bits()),
        );
        assert_ne!(text, tampered, "the fixture must actually change");
        fs::write(&path, tampered).expect("write tampered sidecar");

        let error = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: Some(store.chain_dir_v1()),
            residual_mode: Cycle4M3ResidualModeV1::Centered,
            window_updates: 1,
        })
        .expect_err("a tampered sidecar must fail closed");
        assert_eq!(error.code(), "cycle4_m3_audit_v1_sidecar_validation");
    }

    /// A reordered or spliced Store breaks the declared digest chain before
    /// any residual is read.
    #[test]
    fn a_broken_evidence_chain_fails_closed_v1() {
        let store = TestStoreV1::new_v1("broken-chain");
        let run_sha256 = identity_v1(0x62);
        let updates = vec![
            two_cell_update_v1(&[0.25, 0.5], &[0.125]),
            two_cell_update_v1(&[0.5, 0.25], &[0.375]),
        ];
        write_synthetic_store_v1(&store, &run_sha256, &updates, true);

        edit_continuation_v1(&store, |document| {
            document["update_groups"][1]["previous_update_evidence_sha256"] =
                serde_json::Value::String("ff".repeat(32));
        });

        let error = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: None,
            residual_mode: Cycle4M3ResidualModeV1::Raw,
            window_updates: 1,
        })
        .expect_err("a broken chain must fail closed");
        assert_eq!(error.code(), "cycle4_m3_audit_v1_evidence_chain_broken");
    }

    /// A continuation that is valid JSON but not the Store's canonical form
    /// is refused: the digest is computed over canonical bytes, so a
    /// non-canonical file could not be checked against one.
    #[test]
    fn a_noncanonical_continuation_is_refused_v1() {
        let store = TestStoreV1::new_v1("noncanonical");
        let run_sha256 = identity_v1(0x63);
        let updates = vec![two_cell_update_v1(&[0.25], &[0.125])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, false);

        let path = store
            .store_root_v1()
            .join(SEGMENT_DIRECTORY_V1)
            .join("segment-00000004.continuation-00000000.json");
        let mut bytes = fs::read(&path).expect("read continuation");
        // Drop the trailing LF the canonical form requires.
        bytes.pop();
        fs::write(&path, bytes).expect("write continuation");

        let error = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: None,
            residual_mode: Cycle4M3ResidualModeV1::Raw,
            window_updates: 1,
        })
        .expect_err("a non-canonical continuation must fail closed");
        assert_eq!(
            error.code(),
            "cycle4_m3_audit_v1_continuation_not_canonical"
        );
    }

    /// The report names the checkpoint identity the M2 probe would report for
    /// the same checkpoint, which is what lets routing bind the two.
    #[test]
    fn the_window_names_the_tip_checkpoint_identity_v1() {
        let store = TestStoreV1::new_v1("tip-checkpoint");
        let run_sha256 = identity_v1(0x64);
        let updates = vec![two_cell_update_v1(&[0.25], &[0.125])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, false);

        let window = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: None,
            residual_mode: Cycle4M3ResidualModeV1::Raw,
            window_updates: 1,
        })
        .expect("window");
        let expected = lower_hex_raw32_v1(sha256_v1(
            &fs::read(
                store
                    .store_root_v1()
                    .join(CHECKPOINT_DIRECTORY_V1)
                    .join("update-00000001.checkpoint.json"),
            )
            .expect("read checkpoint"),
        ));
        assert_eq!(window.tip_checkpoint_manifest_sha256, expected);

        fs::remove_file(
            store
                .store_root_v1()
                .join(CHECKPOINT_DIRECTORY_V1)
                .join("update-00000001.checkpoint.json"),
        )
        .expect("remove checkpoint");
        assert_eq!(
            compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                store_root: store.store_root_v1(),
                chain_dir: None,
                residual_mode: Cycle4M3ResidualModeV1::Raw,
                window_updates: 1,
            })
            .expect_err("a window off a checkpoint boundary must fail closed")
            .code(),
            "cycle4_m3_audit_v1_tip_checkpoint_missing"
        );
    }

    #[test]
    fn a_missing_sidecar_and_a_short_store_both_fail_closed_v1() {
        let store = TestStoreV1::new_v1("missing-sidecar");
        let run_sha256 = identity_v1(0x5d);
        let updates = vec![two_cell_update_v1(&[0.2], &[0.1])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, false);

        assert_eq!(
            compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                store_root: store.store_root_v1(),
                chain_dir: Some(store.chain_dir_v1()),
                residual_mode: Cycle4M3ResidualModeV1::Centered,
                window_updates: 1,
            })
            .expect_err("no sidecar must fail closed")
            .code(),
            "cycle4_m3_audit_v1_sidecar_missing"
        );

        assert_eq!(
            compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                store_root: store.store_root_v1(),
                chain_dir: None,
                residual_mode: Cycle4M3ResidualModeV1::Raw,
                window_updates: CYCLE4_M3_WINDOW_UPDATES_V1,
            })
            .expect_err("a one-update store cannot fill a 512-update window")
            .code(),
            "cycle4_m3_audit_v1_window_too_short"
        );

        // Mode/chain pairing is enforced before anything is read.
        assert_eq!(
            compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                store_root: store.store_root_v1(),
                chain_dir: None,
                residual_mode: Cycle4M3ResidualModeV1::Centered,
                window_updates: 1,
            })
            .expect_err("centered without a chain must fail closed")
            .code(),
            "cycle4_m3_audit_v1_chain_dir_required"
        );
        assert_eq!(
            compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                store_root: store.store_root_v1(),
                chain_dir: Some(store.chain_dir_v1()),
                residual_mode: Cycle4M3ResidualModeV1::Raw,
                window_updates: 1,
            })
            .expect_err("raw with a chain must fail closed")
            .code(),
            "cycle4_m3_audit_v1_chain_dir_refused"
        );
    }

    /// Clarification V2.2 (2026-09-05): a search occupant carries no checkpoint
    /// manifest but a search authority and tier; its cell identity is the
    /// documented SHA-256 over both, so every episode has a cell and the
    /// consumers' 64-hex identity handling is unchanged.
    #[test]
    fn a_search_occupant_episode_forms_a_cell_from_its_authority_and_tier_v1() {
        let store = TestStoreV1::new_v1("search-occupant");
        let run_sha256 = identity_v1(0x5f);
        let authority = identity_v1(0x66);
        let mut update = two_cell_update_v1(&[0.2], &[0.1]);
        update[0].search_occupant = Some((authority.clone(), "t2048".to_owned()));
        write_synthetic_store_v1(&store, &run_sha256, &[update], false);

        let window = compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
            store_root: store.store_root_v1(),
            chain_dir: None,
            residual_mode: Cycle4M3ResidualModeV1::Raw,
            window_updates: 1,
        })
        .expect("a search occupant has a cell");
        let expected = search_occupant_cell_identity_v1(&authority, "t2048");
        assert_eq!(expected.len(), 64);
        assert!(expected
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert!(
            window
                .cells()
                .iter()
                .any(|cell| cell.opponent_checkpoint_manifest_sha256 == expected),
            "the derived identity must appear as a cell"
        );
        assert!(
            window
                .cells()
                .iter()
                .all(|cell| cell.opponent_checkpoint_manifest_sha256 != authority),
            "the raw authority hash is never used as the identity"
        );
        // The identity is a pure function of (authority, tier): a different
        // tier is a different cell identity.
        assert_ne!(
            search_occupant_cell_identity_v1(&authority, "t512"),
            expected
        );
    }

    /// A search occupant with a malformed authority (not 64 lower hex) or an
    /// unregistered tier is refused before anything is hashed, with its own
    /// error code, so malformed evidence can never merge or invent cells.
    #[test]
    fn a_malformed_search_occupant_identity_is_refused_not_hashed_v1() {
        for (label, authority, tier) in [
            (
                "colon in authority",
                format!("{}:{}", identity_v1(0x66), "t2048"),
                "t512",
            ),
            (
                "upper hex authority",
                format!("A{}", &identity_v1(0x66)[1..]),
                "t2048",
            ),
            (
                "short authority",
                identity_v1(0x66)[..63].to_owned(),
                "t2048",
            ),
            ("unregistered tier", identity_v1(0x66), "t1024"),
            ("colon in tier", identity_v1(0x66), "t2048:extra"),
        ] {
            let store = TestStoreV1::new_v1(&format!("malformed-{}", label.replace(' ', "-")));
            let run_sha256 = identity_v1(0x5c);
            let mut update = two_cell_update_v1(&[0.2], &[0.1]);
            update[0].search_occupant = Some((authority.clone(), tier.to_owned()));
            write_synthetic_store_v1(&store, &run_sha256, &[update], false);
            assert_eq!(
                compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                    store_root: store.store_root_v1(),
                    chain_dir: None,
                    residual_mode: Cycle4M3ResidualModeV1::Raw,
                    window_updates: 1,
                })
                .expect_err(label)
                .code(),
                "cycle4_m3_audit_v1_malformed_search_occupant_identity",
                "{label}"
            );
        }
    }

    /// A search authority without a tier is not an identity: the audit still
    /// fails closed, with the same code the missing-manifest case reports.
    #[test]
    fn a_search_authority_without_a_tier_still_fails_closed_v1() {
        let store = TestStoreV1::new_v1("search-authority-only");
        let run_sha256 = identity_v1(0x5d);
        let updates = vec![two_cell_update_v1(&[0.2], &[0.1])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, false);

        edit_continuation_v1(&store, |document| {
            let episode = document["update_groups"][0]["evidence"]["episodes"][0]
                .as_object_mut()
                .expect("episode object");
            episode.remove("opponent_checkpoint_manifest_sha256");
            episode.insert(
                "opponent_search_authority_sha256".to_owned(),
                serde_json::Value::String(identity_v1(0x66)),
            );
        });

        assert_eq!(
            compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                store_root: store.store_root_v1(),
                chain_dir: None,
                residual_mode: Cycle4M3ResidualModeV1::Raw,
                window_updates: 1,
            })
            .expect_err("an authority without a tier has no cell")
            .code(),
            "cycle4_m3_audit_v1_episode_without_opponent_identity"
        );
    }

    /// The M3 cell is `(opponent checkpoint identity, learner role)`, so an
    /// episode with no opponent identity has no cell and the audit refuses
    /// rather than inventing one.
    #[test]
    fn an_episode_without_an_opponent_identity_fails_closed_v1() {
        let store = TestStoreV1::new_v1("no-identity");
        let run_sha256 = identity_v1(0x5e);
        let updates = vec![two_cell_update_v1(&[0.2], &[0.1])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, false);

        edit_continuation_v1(&store, |document| {
            document["update_groups"][0]["evidence"]["episodes"][0]
                .as_object_mut()
                .expect("episode object")
                .remove("opponent_checkpoint_manifest_sha256");
        });

        // The structural adaptation runs before the digest recomputation, so
        // an episode with no cell is reported as such rather than as a digest
        // mismatch, which would tell an operator nothing about the defect.
        assert_eq!(
            compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                store_root: store.store_root_v1(),
                chain_dir: None,
                residual_mode: Cycle4M3ResidualModeV1::Raw,
                window_updates: 1,
            })
            .expect_err("a cell-less episode must fail closed")
            .code(),
            "cycle4_m3_audit_v1_episode_without_opponent_identity"
        );
    }

    /// An evidence record carrying a field this build does not know is
    /// rejected: the leaf mirrors are `deny_unknown_fields` on purpose.
    #[test]
    fn an_unknown_episode_field_is_rejected_v1() {
        let store = TestStoreV1::new_v1("unknown-field");
        let run_sha256 = identity_v1(0x5f);
        let updates = vec![two_cell_update_v1(&[0.2], &[0.1])];
        write_synthetic_store_v1(&store, &run_sha256, &updates, false);

        edit_continuation_v1(&store, |document| {
            document["update_groups"][0]["evidence"]["episodes"][0]
                .as_object_mut()
                .expect("episode object")
                .insert("a_field_from_the_future".to_owned(), serde_json::json!(1));
        });

        assert_eq!(
            compute_cycle4_m3_window_v1(&Cycle4M3WindowRequestV1 {
                store_root: store.store_root_v1(),
                chain_dir: None,
                residual_mode: Cycle4M3ResidualModeV1::Raw,
                window_updates: 1,
            })
            .expect_err("an unknown episode field must fail closed")
            .code(),
            "cycle4_m3_audit_v1_evidence_decode"
        );
    }

    /// A report whose verdict does not follow from its own numbers is
    /// refused by the decoder, so the routing selector can never read one.
    #[test]
    fn a_tampered_verdict_is_refused_v1() {
        let reference_bytes = build_cycle4_m3_reference_document_v1(
            &test_support_window_v1(
                Cycle4M3ResidualModeV1::Raw,
                vec![cell_v1(1, "p0", 10_000, -0.008, 1.0)],
                10_000,
                identity_v1(7),
                identity_v1(8),
            ),
            identity_v1(0x0a),
        )
        .expect("reference");
        let reference =
            decode_cycle4_m3_reference_document_v1(&reference_bytes).expect("decode reference");
        let window = test_support_window_v1(
            Cycle4M3ResidualModeV1::Centered,
            vec![cell_v1(1, "p0", 10_000, 0.9, 1.0)],
            10_000,
            identity_v1(3),
            identity_v1(4),
        );
        let (bytes, pass) =
            build_cycle4_m3_audit_report_v1("static-rb", &window, &reference, &identity_v1(0x0d))
                .expect("report");
        assert!(!pass);
        let mut report = decode_cycle4_m3_audit_report_v1(&bytes).expect("decode");
        report.verdict = CYCLE4_M3_VERDICT_PASS_V1.to_owned();
        report.failures.clear();
        let tampered = to_canonical_json_bytes_v1(&report, CanonicalJsonNullPolicyV1::Forbid)
            .expect("re-encode");
        assert_eq!(
            decode_cycle4_m3_audit_report_v1(&tampered)
                .expect_err("tampered verdict must be refused")
                .code(),
            "cycle4_m3_audit_v1_report_verdict"
        );
    }
}
