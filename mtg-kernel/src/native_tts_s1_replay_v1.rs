//! Test-time-search wrapper, stage S1: the per-tier feasibility replay
//! (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S1: the frozen
//! corpus "replayed per tier; records p50/p99/max wall time and decisions
//! per second; a tier failing the SLO or the compute cap is INFEASIBLE").
//!
//! # What "the production selector" means here
//!
//! Every decision is searched through
//! `native_checkpoint_shadow_stdio_v1::model_guided_search_full_budget_v1`,
//! the function the CP7 scorer's own `ModelGuidedSearch` selector calls,
//! reached through
//! `native_checkpoint_shadow_stdio_v1::model_guided_search_pinned_evaluator_v1`,
//! the only constructor of the production leaf evaluator and the site of
//! the MXCSR pin-and-verify gate. The authority is built by
//! `BoundModelGuidedSearchV1::bind_v1`, the same constructor the scorer's
//! `begin_episode_v1` uses, from the same
//! [`ShadowCheckpointIdentityV1`] the same loader produced. So the tier,
//! the transition budget, the depth cap, the checkpoint lineage, the
//! consumption mode, the value domain, and therefore the authority digest
//! and every simulation seed derived from it are the panel's, not a
//! look-alike. None of that is asserted here by comment: it is the same
//! code, called.
//!
//! The two diagnostic stability halves are OFF, per the task's own
//! configuration and Amendment V2.1 ("the two diagnostic stability halves
//! are timed separately and need not run in a formal panel"): S1 measures
//! the product latency a formal panel would pay, and the halves roughly
//! triple it.
//!
//! # Reconstruction is a replay, never a restore
//!
//! A corpus decision is reached by resetting the episode from its recorded
//! base seed and episode id and replaying its recorded flat-action
//! sequence through the kernel. Nothing about the recorded state is
//! trusted: before any search runs, the reconstructed decision's whole
//! legal surface (the decision identity fields, the legal-action count,
//! the diagnostic state hash, and the privileged core environment hash,
//! which itself folds the ordered legal-action semantics) must equal what
//! the corpus recorded. A mismatch is
//! [`TtsS1ReplayErrorV1::ReconstructionMismatch`] and stops the run; it is
//! never a warning, and never a decision measured against a different
//! state than the one it was selected from.
//!
//! Decisions from one episode are walked in ascending ordinal on a single
//! session rather than re-reset per decision, which is sound because the
//! search cannot mutate the session it searches (`select_action_v1` takes
//! `&FastActorSessionV1` and re-verifies the authoritative environment
//! hash on the way out) and because every target decision still gets its
//! own full surface check.
//!
//! # Timing can never reach a chosen action
//!
//! Owner law. Every clock in this module is read AFTER the value it
//! annotates is already fixed, and no branch anywhere reads a duration.
//! The ceiling classification reuses
//! `CeilingStatusV4::classify_v4`, so the 4.0 s SLO and the 20.0 s hard
//! timeout are the pre-registered constants themselves rather than copies
//! of them. [`strip_timing_fields_v1`] exists so the claim is testable in
//! one move, exactly as the S0 replay test strips `wall_time`.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonNullPolicyV1,
};
use crate::fast_sampler::FastCategoricalScratch;
use crate::kernel_native_search_opponent_v1::KernelNativeSearchTierV1;
use crate::model_guided_search_authority_v1::authorized_seed_block_v1;
use crate::model_guided_search_contract_digests_v1::MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1;
use crate::model_guided_search_core_v1::ModelGuidedSearchLeafCensusV1;
use crate::model_guided_search_outcome_v4::{
    lower_hex_sha256_v4, root_statistics_digest_v4, visit_margin_v4, CeilingStatusV4,
    WrapperIdentityV4, MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4,
    MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4, MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V4,
};
use crate::native_checkpoint_shadow_stdio_v1::{
    load_checkpoint_v1, model_guided_search_full_budget_v1,
    model_guided_search_pinned_evaluator_v1, BoundModelGuidedSearchV1, ShadowCheckpointAuthorityV1,
    ShadowCheckpointIdentityV1,
};
use crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1;
use crate::native_tts_s1_corpus_v1::{
    corpus_policy_sample_seed_v1, decode_tts_s1_corpus_v1, publish_canonical_document_v1,
    TtsS1CorpusCheckpointV1, TtsS1CorpusDecisionV1, TtsS1CorpusErrorV1, TtsS1CorpusManifestV1,
    TtsS1DecisionScorerV1,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, CANONICAL_RALLY_DECK_ID,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Wire schema of the per-tier feasibility report.
pub const TTS_S1_REPLAY_REPORT_SCHEMA_V1: &str = "mtg-kernel-tts-s1-replay-report/v1";

/// How the two percentiles are defined, stated on the wire so nobody has
/// to guess which of the several common conventions produced them.
pub const TTS_S1_PERCENTILE_RULE_V1: &str =
    "nearest-rank-on-ascending-integer-microseconds-rank-equals-ceil-p-times-n-over-100/v1";

/// The per-decision chain's genesis. Reuses the outcome-V4 genesis
/// spelling so an auditor reading both artifacts sees one convention.
pub const TTS_S1_REPLAY_CHAIN_GENESIS_V1: &str = MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V4;

/// Micros per second, as the integer scale every duration on the wire uses.
const MICROS_PER_SECOND_V1: u64 = 1_000_000;

/// Whether the tier may be measured against CP7 on latency grounds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsS1TierVerdictV1 {
    Feasible,
    Infeasible,
}

impl TtsS1TierVerdictV1 {
    pub const fn tag_v1(self) -> &'static str {
        match self {
            Self::Feasible => "FEASIBLE",
            Self::Infeasible => "INFEASIBLE",
        }
    }
}

/// The two timings this stage exists to measure, grouped so a test (or an
/// auditor) can excise them in one move.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1DecisionWallTimeV1 {
    /// The search alone: the one `select_action_v1` call, nothing else.
    pub search_micros: u64,
    /// The whole decision as a panel would pay for it: the flat encode,
    /// the tensorization, the policy forward, the policy sample, the
    /// search, and this record's own construction. It deliberately
    /// EXCLUDES the fast-forward that repositioned the session, which is
    /// an artifact of replaying a corpus and is not work any live decision
    /// does.
    pub decision_micros: u64,
}

/// Where each of the two timings landed against the pre-registered
/// ceilings. Recorded, never acted on.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1DecisionCeilingsV1 {
    pub search: CeilingStatusV4,
    pub decision: CeilingStatusV4,
}

/// One replayed decision, hash-chained to its predecessor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ReplayDecisionRecordV1 {
    pub record_ordinal: u64,
    /// [`TTS_S1_REPLAY_CHAIN_GENESIS_V1`] for ordinal 0, otherwise the
    /// SHA-256 of the previous record's own canonical JSON bytes.
    pub previous_record_sha256: String,
    pub episode_id: u64,
    pub decision_ordinal: u64,
    pub acting_player: PlayerSeatV1,
    pub legal_action_count: u32,
    /// What the search chose. The claim S1 has to keep is that this is a
    /// function of (checkpoint, corpus coordinates, authority) and of
    /// nothing else, timing included.
    pub chosen_action_index: u32,
    /// What the temperature-1 policy sample would have chosen at this
    /// decision under the corpus's own sampling seed.
    pub policy_sample_index: u32,
    pub search_overrode_policy_sample: bool,
    pub requested_transitions: u32,
    /// Kernel transitions the search actually consumed.
    pub actual_transitions: u32,
    pub simulations: u32,
    pub tree_node_count: u32,
    /// Terminal-leaf counts and cutoff depth, per the sketch's S1 record.
    pub leaf_census: ModelGuidedSearchLeafCensusV1,
    pub root_statistics_digest_sha256: String,
    pub visit_margin: u32,
    pub wall_time: TtsS1DecisionWallTimeV1,
    pub ceilings: TtsS1DecisionCeilingsV1,
}

impl TtsS1ReplayDecisionRecordV1 {
    /// The chain link this record contributes: SHA-256 over its own
    /// canonical JSON bytes, which already end in the single LF the
    /// canonical codec appends.
    pub fn chain_link_v1(&self) -> Result<[u8; 32], TtsS1ReplayErrorV1> {
        let bytes = to_canonical_json_bytes_v1(self, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1ReplayErrorV1::CanonicalJson)?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(hasher.finalize().into())
    }
}

/// p50, p99, max and the total, all in integer microseconds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1TimingSummaryV1 {
    pub p50_micros: u64,
    pub p99_micros: u64,
    pub max_micros: u64,
    pub total_micros: u64,
}

/// Nearest-rank percentile over ascending integer microseconds.
///
/// Rank is `ceil(percentile * n / 100)`, clamped to `1..=n`, and the
/// result is the value at that 1-based rank. Integer arithmetic
/// throughout: no interpolation, no float, no rounding mode to argue
/// about later. `samples` must already be sorted ascending.
pub fn nearest_rank_percentile_micros_v1(samples: &[u64], percentile: u64) -> Option<u64> {
    let count = u64::try_from(samples.len()).ok()?;
    if count == 0 {
        return None;
    }
    let rank = percentile.checked_mul(count)?.div_ceil(100).clamp(1, count);
    samples.get(usize::try_from(rank - 1).ok()?).copied()
}

/// Builds the summary from an UNSORTED sample set.
pub fn summarize_micros_v1(samples: &[u64]) -> Option<TtsS1TimingSummaryV1> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    Some(TtsS1TimingSummaryV1 {
        p50_micros: nearest_rank_percentile_micros_v1(&sorted, 50)?,
        p99_micros: nearest_rank_percentile_micros_v1(&sorted, 99)?,
        max_micros: *sorted.last()?,
        total_micros: sorted
            .iter()
            .fold(0u64, |total, value| total.saturating_add(*value)),
    })
}

/// Everything the report digest covers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ReplayReportBodyV1 {
    pub engine_commit: String,
    /// The tier tag, in the ladder's own spelling (`t512`, ...).
    pub tier: String,
    pub transition_budget: u32,
    pub policy_step_depth_cap: u16,
    pub seed_block_id: u64,
    pub seed_block_seed: u64,
    /// Always false in S1; recorded because the number means something
    /// different if it were ever true.
    pub stability_halves_enabled: bool,
    pub checkpoint: TtsS1CorpusCheckpointV1,
    /// The exact wrapper identity a CP7 panel record would carry for this
    /// configuration, built by the scorer's own constructor.
    pub wrapper_identity: WrapperIdentityV4,
    pub search_authority_digest_sha256: String,
    /// The frozen corpus this report measured.
    pub corpus_sha256: String,
    pub corpus_decision_count: u64,
    pub decisions_replayed: u64,
    /// False when `--limit-decisions` cut the run short. A partial run is
    /// a smoke, never a feasibility verdict a panel may rely on, and the
    /// verdict below says so.
    pub replayed_whole_corpus: bool,
    pub percentile_rule: String,
    pub search_wall_time: TtsS1TimingSummaryV1,
    pub decision_wall_time: TtsS1TimingSummaryV1,
    /// Throughput as decisions per second times 1,000, floored. Scaled to
    /// an integer because the canonical JSON codec forbids floats
    /// outright; the two operands are on the wire beside it, so the exact
    /// rational is recoverable.
    pub decisions_per_second_milli: u64,
    pub slo_micros: u64,
    pub hard_timeout_micros: u64,
    pub p99_decision_ceiling_status: CeilingStatusV4,
    pub max_decision_ceiling_status: CeilingStatusV4,
    pub verdict: TtsS1TierVerdictV1,
    pub verdict_reason: String,
    pub chain_genesis_sha256: String,
    /// SHA-256 of the last decision record's canonical bytes, so the whole
    /// chain is committed to by one field.
    pub final_record_sha256: String,
    pub decisions: Vec<TtsS1ReplayDecisionRecordV1>,
}

/// The published report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ReplayReportV1 {
    pub schema: String,
    /// SHA-256 over the canonical JSON bytes of `body`, lower hex.
    pub report_sha256: String,
    pub body: TtsS1ReplayReportBodyV1,
}

impl TtsS1ReplayReportV1 {
    pub fn seal_v1(body: TtsS1ReplayReportBodyV1) -> Result<Self, TtsS1ReplayErrorV1> {
        let bytes = to_canonical_json_bytes_v1(&body, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1ReplayErrorV1::CanonicalJson)?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest: [u8; 32] = hasher.finalize().into();
        Ok(Self {
            schema: TTS_S1_REPLAY_REPORT_SCHEMA_V1.to_owned(),
            report_sha256: lower_hex_sha256_v4(digest),
            body,
        })
    }

    pub fn canonical_bytes_v1(&self) -> Result<Vec<u8>, TtsS1ReplayErrorV1> {
        to_canonical_json_bytes_v1(self, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1ReplayErrorV1::CanonicalJson)
    }
}

/// Re-proves a published report: canonical bytes, schema, own digest, and
/// the full per-decision chain.
pub fn decode_tts_s1_replay_report_v1(
    bytes: &[u8],
) -> Result<TtsS1ReplayReportV1, TtsS1ReplayErrorV1> {
    let report: TtsS1ReplayReportV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1ReplayErrorV1::InvalidReport)?;
    let reencoded = to_canonical_json_bytes_v1(&report, CanonicalJsonNullPolicyV1::Forbid)
        .map_err(|_| TtsS1ReplayErrorV1::CanonicalJson)?;
    let resealed = TtsS1ReplayReportV1::seal_v1(report.body.clone())?;
    if reencoded != bytes
        || report.schema != TTS_S1_REPLAY_REPORT_SCHEMA_V1
        || resealed.report_sha256 != report.report_sha256
    {
        return Err(TtsS1ReplayErrorV1::InvalidReport);
    }
    verify_tts_s1_replay_chain_v1(&report.body)?;
    Ok(report)
}

/// Walks the per-decision hash chain. Returns the record count.
pub fn verify_tts_s1_replay_chain_v1(
    body: &TtsS1ReplayReportBodyV1,
) -> Result<usize, TtsS1ReplayErrorV1> {
    if body.chain_genesis_sha256 != TTS_S1_REPLAY_CHAIN_GENESIS_V1 {
        return Err(TtsS1ReplayErrorV1::BrokenChain);
    }
    let mut expected_previous = TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned();
    for (index, record) in body.decisions.iter().enumerate() {
        if record.record_ordinal != index as u64
            || record.previous_record_sha256 != expected_previous
        {
            return Err(TtsS1ReplayErrorV1::BrokenChain);
        }
        expected_previous = lower_hex_sha256_v4(record.chain_link_v1()?);
    }
    if body.decisions.is_empty() || body.final_record_sha256 != expected_previous {
        return Err(TtsS1ReplayErrorV1::BrokenChain);
    }
    if body.decisions.len() as u64 != body.decisions_replayed {
        return Err(TtsS1ReplayErrorV1::BrokenChain);
    }
    Ok(body.decisions.len())
}

/// Parses a report and removes every field that a re-run is allowed to
/// change: the two timings, their ceiling classifications, the chain (whose
/// links cover those timings), and the derived verdict and digests.
///
/// What survives is exactly the substantive claim S1 makes about the
/// wrapper: the same corpus decisions, in the same order, with the same
/// chosen actions and the same search products. Two runs of the same tier
/// against the same corpus must produce equal strippings; this is the S0
/// `strip_wall_time_v1` pattern applied to a whole document instead of a
/// JSONL line.
pub fn strip_timing_fields_v1(bytes: &[u8]) -> Result<serde_json::Value, TtsS1ReplayErrorV1> {
    let mut value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| TtsS1ReplayErrorV1::InvalidReport)?;
    let root = value
        .as_object_mut()
        .ok_or(TtsS1ReplayErrorV1::InvalidReport)?;
    root.remove("report_sha256");
    let body = root
        .get_mut("body")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or(TtsS1ReplayErrorV1::InvalidReport)?;
    for key in [
        "search_wall_time",
        "decision_wall_time",
        "decisions_per_second_milli",
        "p99_decision_ceiling_status",
        "max_decision_ceiling_status",
        "verdict",
        "verdict_reason",
        "final_record_sha256",
    ] {
        body.remove(key);
    }
    let decisions = body
        .get_mut("decisions")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or(TtsS1ReplayErrorV1::InvalidReport)?;
    for decision in decisions.iter_mut() {
        let record = decision
            .as_object_mut()
            .ok_or(TtsS1ReplayErrorV1::InvalidReport)?;
        record.remove("wall_time");
        record.remove("ceilings");
        record.remove("previous_record_sha256");
    }
    Ok(value)
}

/// Fail-closed error vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TtsS1ReplayErrorV1 {
    UnauthorizedSeedBlock,
    Corpus(TtsS1CorpusErrorV1),
    CorpusRead(String),
    CheckpointLoad(String),
    /// The corpus was built against a different checkpoint than the one
    /// this replay loaded.
    CheckpointMismatch,
    /// The corpus decisions are not in ascending (episode id, decision
    /// ordinal) order, or a decision's action prefix disagrees with an
    /// earlier decision's from the same episode.
    CorpusOrder,
    EpisodeSchedule,
    SessionReset(String),
    /// The reconstruction reached a terminal, or a different decision,
    /// where the corpus recorded one.
    ReconstructionMismatch {
        episode_id: u64,
        decision_ordinal: u64,
        field: &'static str,
    },
    Binding(String),
    Consume(String),
    Encode,
    Score,
    ScoreContract,
    Sample,
    /// The scorer's own fail-closed selector error, verbatim.
    Search(&'static str),
    /// `--limit-decisions 0`, or an empty corpus.
    NoDecisions,
    CanonicalJson,
    InvalidReport,
    BrokenChain,
    Publication(String),
}

impl TtsS1ReplayErrorV1 {
    pub fn code_v1(&self) -> &'static str {
        match self {
            Self::UnauthorizedSeedBlock => "tts_s1_replay_unauthorized_seed_block",
            Self::Corpus(_) => "tts_s1_replay_corpus_invalid",
            Self::CorpusRead(_) => "tts_s1_replay_corpus_unreadable",
            Self::CheckpointLoad(_) => "tts_s1_replay_checkpoint_load_failed",
            Self::CheckpointMismatch => "tts_s1_replay_checkpoint_mismatch",
            Self::CorpusOrder => "tts_s1_replay_corpus_order_invalid",
            Self::EpisodeSchedule => "tts_s1_replay_episode_schedule_invalid",
            Self::SessionReset(_) => "tts_s1_replay_session_reset_failed",
            Self::ReconstructionMismatch { .. } => "tts_s1_replay_reconstruction_mismatch",
            Self::Binding(_) => "tts_s1_replay_action_binding_failed",
            Self::Consume(_) => "tts_s1_replay_action_consume_failed",
            Self::Encode => "tts_s1_replay_decision_encoding_failed",
            Self::Score => "tts_s1_replay_checkpoint_scoring_failed",
            Self::ScoreContract => "tts_s1_replay_checkpoint_score_invalid",
            Self::Sample => "tts_s1_replay_policy_sampling_failed",
            Self::Search(_) => "tts_s1_replay_search_failed",
            Self::NoDecisions => "tts_s1_replay_no_decisions",
            Self::CanonicalJson => "tts_s1_replay_canonical_json_failed",
            Self::InvalidReport => "tts_s1_replay_report_invalid",
            Self::BrokenChain => "tts_s1_replay_chain_broken",
            Self::Publication(_) => "tts_s1_replay_publication_failed",
        }
    }
}

impl fmt::Display for TtsS1ReplayErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corpus(error) => write!(formatter, "{}: {error}", self.code_v1()),
            Self::CorpusRead(detail)
            | Self::CheckpointLoad(detail)
            | Self::SessionReset(detail)
            | Self::Binding(detail)
            | Self::Consume(detail)
            | Self::Publication(detail) => write!(formatter, "{}: {detail}", self.code_v1()),
            Self::Search(detail) => write!(formatter, "{}: {detail}", self.code_v1()),
            Self::ReconstructionMismatch {
                episode_id,
                decision_ordinal,
                field,
            } => write!(
                formatter,
                "{}: episode {episode_id} decision {decision_ordinal} field {field}",
                self.code_v1()
            ),
            _ => formatter.write_str(self.code_v1()),
        }
    }
}

impl std::error::Error for TtsS1ReplayErrorV1 {}

impl From<TtsS1CorpusErrorV1> for TtsS1ReplayErrorV1 {
    fn from(error: TtsS1CorpusErrorV1) -> Self {
        Self::Corpus(error)
    }
}

/// The tier ladder, spelled exactly as the sketch pre-registers it. No
/// alias, no numeric form, no case folding: a typo must be a usage error,
/// never a neighbouring tier.
pub fn parse_tts_s1_tier_v1(text: &str) -> Option<KernelNativeSearchTierV1> {
    match text {
        "t512" => Some(KernelNativeSearchTierV1::T512),
        "t2048" => Some(KernelNativeSearchTierV1::T2048),
        "t8192" => Some(KernelNativeSearchTierV1::T8192),
        "t32768" => Some(KernelNativeSearchTierV1::T32768),
        _ => None,
    }
}

/// Everything the launcher chooses for one tier's replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsS1ReplayConfigV1 {
    pub authority: ShadowCheckpointAuthorityV1,
    pub corpus_path: PathBuf,
    pub tier: KernelNativeSearchTierV1,
    /// Index into `MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1`. The
    /// wrapper's own action seed, independent of the corpus's.
    pub seed_block_id: usize,
    /// Smoke bound. `None` replays the whole corpus, which is the only
    /// configuration whose verdict a panel may rely on.
    pub limit_decisions: Option<u64>,
}

/// Reconstructs, searches, times, and reports one tier over one corpus.
pub fn run_tts_s1_replay_v1(
    config: &TtsS1ReplayConfigV1,
) -> Result<TtsS1ReplayReportV1, TtsS1ReplayErrorV1> {
    let corpus_bytes = std::fs::read(&config.corpus_path)
        .map_err(|error| TtsS1ReplayErrorV1::CorpusRead(error.to_string()))?;
    let corpus = decode_tts_s1_corpus_v1(&corpus_bytes)?;
    let action_seed = authorized_seed_block_v1(config.seed_block_id)
        .ok_or(TtsS1ReplayErrorV1::UnauthorizedSeedBlock)?;
    let loaded = load_checkpoint_v1(config.authority.clone())
        .map_err(|error| TtsS1ReplayErrorV1::CheckpointLoad(error.to_string()))?;
    let architecture = loaded
        .inference
        .search_model_v1()
        .architecture_identity_v1()
        .to_owned();
    let checkpoint = TtsS1CorpusCheckpointV1::from_identity_v1(&loaded.identity, &architecture);
    let body = replay_corpus_body_v1(
        &loaded.inference,
        &loaded.identity,
        &architecture,
        checkpoint,
        &corpus,
        config.tier,
        config.seed_block_id,
        action_seed,
        config.limit_decisions,
    )?;
    TtsS1ReplayReportV1::seal_v1(body)
}

/// The replay itself, over the narrow model seam.
///
/// Separated from [`run_tts_s1_replay_v1`] so this crate's own tests can
/// drive the whole reconstruct-verify-search-record path against the
/// in-memory runner-fixed net, with no Store on disk, exactly as the S0
/// search tests do.
#[allow(clippy::too_many_arguments)]
pub(crate) fn replay_corpus_body_v1(
    scorer: &dyn TtsS1DecisionScorerV1,
    identity: &ShadowCheckpointIdentityV1,
    architecture: &str,
    checkpoint: TtsS1CorpusCheckpointV1,
    corpus: &TtsS1CorpusManifestV1,
    tier: KernelNativeSearchTierV1,
    seed_block_id: usize,
    action_seed: u64,
    limit_decisions: Option<u64>,
) -> Result<TtsS1ReplayReportBodyV1, TtsS1ReplayErrorV1> {
    // The corpus names the checkpoint it was drawn from; measuring a
    // different one would produce a real report about a population that
    // does not exist.
    if checkpoint != corpus.body.checkpoint {
        return Err(TtsS1ReplayErrorV1::CheckpointMismatch);
    }

    let corpus_decision_count = corpus.body.decisions.len() as u64;
    let budget = limit_decisions.unwrap_or(corpus_decision_count);
    if budget == 0 || corpus_decision_count == 0 {
        return Err(TtsS1ReplayErrorV1::NoDecisions);
    }
    let planned = budget.min(corpus_decision_count);

    let value_domain = MODEL_GUIDED_SEARCH_WRAPPER_VALUE_DOMAIN_V1;
    let mut bound: Option<BoundModelGuidedSearchV1> = None;
    let mut records: Vec<TtsS1ReplayDecisionRecordV1> = Vec::new();
    let mut search_samples: Vec<u64> = Vec::new();
    let mut decision_samples: Vec<u64> = Vec::new();
    let mut previous_record_sha256 = TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned();

    let mut cursor = 0usize;
    let targets = &corpus.body.decisions;
    while cursor < targets.len() && (records.len() as u64) < planned {
        let episode_id = targets[cursor].coordinates.episode_id;
        let mut session = reset_episode_v1(
            &targets[cursor],
            corpus.body.max_physical_decisions,
            corpus.body.max_policy_steps,
        )?;
        let mut applied: Vec<u32> = Vec::new();

        while cursor < targets.len()
            && targets[cursor].coordinates.episode_id == episode_id
            && (records.len() as u64) < planned
        {
            let target = &targets[cursor];
            let ordinal = target.coordinates.decision_ordinal;
            if (ordinal as usize) < applied.len()
                || target.coordinates.action_sequence.len() != ordinal as usize
                || target.coordinates.action_sequence[..applied.len()] != applied[..]
            {
                return Err(TtsS1ReplayErrorV1::CorpusOrder);
            }
            while applied.len() < ordinal as usize {
                let expected = live_decision_v1(&session, target, "fast_forward_terminal")?;
                let binding = session
                    .native_full_trajectory_current_binding_v2(expected)
                    .map_err(|error| TtsS1ReplayErrorV1::Binding(format!("{error:?}")))?;
                let action = target.coordinates.action_sequence[applied.len()];
                session
                    .consume_current_flat_action_slice_v2(binding, action)
                    .map_err(|error| TtsS1ReplayErrorV1::Consume(format!("{:?}", error.code)))?;
                applied.push(action);
            }

            let expected = live_decision_v1(&session, target, "target_terminal")?;
            verify_reconstructed_surface_v1(&session, expected, target)?;

            if bound.is_none() {
                bound = Some(
                    BoundModelGuidedSearchV1::bind_v1(
                        tier,
                        seed_block_id,
                        action_seed,
                        &value_domain,
                        session.kernel_search_private_diagnostic_identity_v1(),
                        identity,
                        architecture,
                    )
                    .map_err(TtsS1ReplayErrorV1::Search)?,
                );
            }
            let bound_ref = bound.as_ref().ok_or(TtsS1ReplayErrorV1::Search(
                "model_guided_search_authority_unbound",
            ))?;
            if bound_ref.private_diagnostic_identity
                != session.kernel_search_private_diagnostic_identity_v1()
            {
                return Err(TtsS1ReplayErrorV1::Search(
                    "model_guided_search_diagnostic_identity_changed",
                ));
            }

            let record = search_one_decision_v1(
                scorer,
                bound_ref,
                &value_domain,
                &session,
                expected,
                target,
                records.len() as u64,
                previous_record_sha256.clone(),
            )?;
            search_samples.push(record.wall_time.search_micros);
            decision_samples.push(record.wall_time.decision_micros);
            previous_record_sha256 = lower_hex_sha256_v4(record.chain_link_v1()?);
            records.push(record);
            cursor += 1;
        }

        // The inner loop leaves `cursor` either at the first decision of
        // the next episode (this episode is finished) or at an unreplayed
        // decision of this one (the smoke budget ran out). The outer
        // loop's own budget test covers the second case, so nothing more
        // is needed here to terminate.
    }

    let decisions_replayed = records.len() as u64;
    if decisions_replayed == 0 {
        return Err(TtsS1ReplayErrorV1::NoDecisions);
    }
    let search_wall_time =
        summarize_micros_v1(&search_samples).ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
    let decision_wall_time =
        summarize_micros_v1(&decision_samples).ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
    let decisions_per_second_milli = if decision_wall_time.total_micros == 0 {
        0
    } else {
        decisions_replayed
            .saturating_mul(MICROS_PER_SECOND_V1)
            .saturating_mul(1_000)
            / decision_wall_time.total_micros
    };
    let p99_decision_ceiling_status = classify_micros_v1(decision_wall_time.p99_micros);
    let max_decision_ceiling_status = classify_micros_v1(decision_wall_time.max_micros);
    let replayed_whole_corpus = decisions_replayed == corpus_decision_count;
    let (verdict, verdict_reason) = verdict_v1(
        p99_decision_ceiling_status,
        max_decision_ceiling_status,
        replayed_whole_corpus,
    );

    let bound_ref = bound.as_ref().ok_or(TtsS1ReplayErrorV1::Search(
        "model_guided_search_authority_unbound",
    ))?;
    let body = TtsS1ReplayReportBodyV1 {
        engine_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_owned(),
        tier: bound_ref.wrapper_identity.tier.clone(),
        transition_budget: bound_ref.wrapper_identity.transition_budget,
        policy_step_depth_cap: bound_ref.wrapper_identity.policy_step_depth_cap,
        seed_block_id: seed_block_id as u64,
        seed_block_seed: action_seed,
        stability_halves_enabled: false,
        checkpoint,
        wrapper_identity: bound_ref.wrapper_identity.clone(),
        search_authority_digest_sha256: bound_ref.authority_digest_sha256.clone(),
        corpus_sha256: corpus.corpus_sha256.clone(),
        corpus_decision_count,
        decisions_replayed,
        replayed_whole_corpus,
        percentile_rule: TTS_S1_PERCENTILE_RULE_V1.to_owned(),
        search_wall_time,
        decision_wall_time,
        decisions_per_second_milli,
        slo_micros: slo_micros_v1(),
        hard_timeout_micros: hard_timeout_micros_v1(),
        p99_decision_ceiling_status,
        max_decision_ceiling_status,
        verdict,
        verdict_reason,
        chain_genesis_sha256: TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned(),
        final_record_sha256: previous_record_sha256,
        decisions: records,
    };
    verify_tts_s1_replay_chain_v1(&body)?;
    Ok(body)
}

/// The SLO in the same integer microseconds the report uses, derived from
/// the pre-registered seconds constant rather than restated.
pub fn slo_micros_v1() -> u64 {
    (MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4 * MICROS_PER_SECOND_V1 as f64) as u64
}

/// The hard protocol timeout, likewise derived.
pub fn hard_timeout_micros_v1() -> u64 {
    (MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4 * MICROS_PER_SECOND_V1 as f64) as u64
}

/// Classifies an integer-microsecond duration through the pre-registered
/// V4 classifier, so the two thresholds can never drift from the panel's.
pub fn classify_micros_v1(micros: u64) -> CeilingStatusV4 {
    CeilingStatusV4::classify_v4(micros as f64 / MICROS_PER_SECOND_V1 as f64)
}

/// The tier verdict and the sentence that explains it.
///
/// INFEASIBLE whenever the p99 decision time is not inside the SLO, or the
/// slowest decision reached the hard protocol timeout (inside a formal
/// panel that is a product failure of the panel, so a tier that can
/// produce one is not admissible), or the run did not cover the whole
/// corpus (a partial run has no feasibility standing at all). Every branch
/// produces a report; none of them is silent.
pub fn verdict_v1(
    p99: CeilingStatusV4,
    max: CeilingStatusV4,
    replayed_whole_corpus: bool,
) -> (TtsS1TierVerdictV1, String) {
    if !replayed_whole_corpus {
        return (
            TtsS1TierVerdictV1::Infeasible,
            "partial replay: a feasibility verdict requires the whole frozen corpus".to_owned(),
        );
    }
    match (p99, max) {
        (CeilingStatusV4::WithinSlo, CeilingStatusV4::HardTimeoutExceeded) => (
            TtsS1TierVerdictV1::Infeasible,
            format!(
                "p99 decision wall time is inside the {MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4} s SLO but the slowest decision reached the {MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4} s hard protocol timeout"
            ),
        ),
        (CeilingStatusV4::WithinSlo, _) => (
            TtsS1TierVerdictV1::Feasible,
            format!(
                "p99 decision wall time is inside the {MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4} s SLO and no decision reached the {MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4} s hard protocol timeout"
            ),
        ),
        _ => (
            TtsS1TierVerdictV1::Infeasible,
            format!(
                "p99 decision wall time exceeds the {MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4} s SLO"
            ),
        ),
    }
}

fn reset_episode_v1(
    target: &TtsS1CorpusDecisionV1,
    max_physical_decisions: u64,
    max_policy_steps: u64,
) -> Result<FastActorSessionV1, TtsS1ReplayErrorV1> {
    let coordinates = &target.coordinates;
    let schedule =
        native_trainer_episode_schedule_v1(coordinates.episode_base_seed, coordinates.episode_id)
            .map_err(|_| TtsS1ReplayErrorV1::EpisodeSchedule)?;
    if schedule.environment_seed != coordinates.environment_seed {
        return Err(TtsS1ReplayErrorV1::ReconstructionMismatch {
            episode_id: coordinates.episode_id,
            decision_ordinal: coordinates.decision_ordinal,
            field: "environment_seed",
        });
    }
    FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
        coordinates.episode_id,
        schedule.environment_seed,
        max_physical_decisions,
        max_policy_steps,
        [
            CANONICAL_RALLY_DECK_ID.to_owned(),
            CANONICAL_RALLY_DECK_ID.to_owned(),
        ],
    )
    .map_err(|error| TtsS1ReplayErrorV1::SessionReset(format!("{:?}", error.code)))
}

fn live_decision_v1(
    session: &FastActorSessionV1,
    target: &TtsS1CorpusDecisionV1,
    field: &'static str,
) -> Result<FastActorDecisionV1, TtsS1ReplayErrorV1> {
    match session.current_response() {
        FastActorResponseV1::Decision(decision) => Ok(decision),
        FastActorResponseV1::Terminal(_) => Err(TtsS1ReplayErrorV1::ReconstructionMismatch {
            episode_id: target.coordinates.episode_id,
            decision_ordinal: target.coordinates.decision_ordinal,
            field,
        }),
    }
}

/// The fail-closed surface check. Every recorded field of the legal
/// surface must match before a search is allowed to run.
fn verify_reconstructed_surface_v1(
    session: &FastActorSessionV1,
    expected: FastActorDecisionV1,
    target: &TtsS1CorpusDecisionV1,
) -> Result<(), TtsS1ReplayErrorV1> {
    let recorded = &target.surface;
    let mismatch = |field: &'static str| TtsS1ReplayErrorV1::ReconstructionMismatch {
        episode_id: target.coordinates.episode_id,
        decision_ordinal: target.coordinates.decision_ordinal,
        field,
    };
    if expected.episode_id != target.coordinates.episode_id {
        return Err(mismatch("episode_id"));
    }
    if expected.step != recorded.policy_step {
        return Err(mismatch("policy_step"));
    }
    if expected.environment_revision != recorded.environment_revision {
        return Err(mismatch("environment_revision"));
    }
    if expected.physical_decision_id != recorded.physical_decision_id {
        return Err(mismatch("physical_decision_id"));
    }
    if expected.substep_index != recorded.substep_index {
        return Err(mismatch("substep_index"));
    }
    if expected.substep_count != recorded.substep_count {
        return Err(mismatch("substep_count"));
    }
    if expected.acting_player != recorded.acting_player {
        return Err(mismatch("acting_player"));
    }
    if expected.legal_action_count != recorded.legal_action_count {
        return Err(mismatch("legal_action_count"));
    }
    if format!("{:016x}", session.diagnostic_state_hash()) != recorded.diagnostic_state_hash_u64_hex
    {
        return Err(mismatch("diagnostic_state_hash"));
    }
    // Folds the ordered legal-action semantics, so this is the legal
    // SURFACE check and not merely a width check.
    if format!("{:016x}", session.privileged_core_environment_hash())
        != recorded.core_environment_hash_u64_hex
    {
        return Err(mismatch("core_environment_hash"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn search_one_decision_v1(
    scorer: &dyn TtsS1DecisionScorerV1,
    bound: &BoundModelGuidedSearchV1,
    value_domain: &crate::model_guided_search_value_quantization_v1::ModelGuidedSearchValueHeadDomainV1,
    session: &FastActorSessionV1,
    expected: FastActorDecisionV1,
    target: &TtsS1CorpusDecisionV1,
    record_ordinal: u64,
    previous_record_sha256: String,
) -> Result<TtsS1ReplayDecisionRecordV1, TtsS1ReplayErrorV1> {
    use crate::async_flat_scored_rollout_v1::FlatScoredFamilyCore;
    use crate::async_flat_scored_rollout_v2::{FlatScoredFamilyV2, OwnedFlatScoringDecisionV2};
    use crate::flat_policy_v2::FlatDecisionEncoderV2;

    // The decision clock opens exactly where a live decision's own work
    // opens: the flat encode. The fast-forward that repositioned the
    // session is outside it, deliberately.
    let decision_started = Instant::now();
    let logits = {
        let mut encoder = FlatDecisionEncoderV2::default();
        let packet = FlatScoredFamilyV2::encode_packet(
            session,
            expected,
            &mut encoder,
            OwnedFlatScoringDecisionV2::default(),
        )
        .map_err(|()| TtsS1ReplayErrorV1::Encode)?;
        let logits = {
            let view = FlatScoredFamilyV2::packet_view(&packet);
            scorer
                .action_logits_v1(view)
                .map_err(|()| TtsS1ReplayErrorV1::Score)?
        };
        drop(FlatScoredFamilyV2::into_owned_packet(packet));
        logits
    };
    if logits.len() != expected.legal_action_count as usize
        || logits.iter().any(|value| !value.is_finite())
    {
        return Err(TtsS1ReplayErrorV1::ScoreContract);
    }
    let policy_sample = FastCategoricalScratch::default()
        .sample(
            &logits,
            corpus_policy_sample_seed_v1(
                target.coordinates.episode_base_seed,
                target.coordinates.episode_id,
                expected,
            ),
        )
        .map_err(|_| TtsS1ReplayErrorV1::Sample)?;
    let policy_sample = u32::try_from(policy_sample).map_err(|_| TtsS1ReplayErrorV1::Sample)?;

    let net = scorer.search_net_v1();
    let evaluator = model_guided_search_pinned_evaluator_v1(net, *value_domain)
        .map_err(TtsS1ReplayErrorV1::Search)?;
    let search_started = Instant::now();
    let full = model_guided_search_full_budget_v1(
        &bound.core,
        &evaluator,
        value_domain,
        session,
        expected,
    )
    .map_err(TtsS1ReplayErrorV1::Search)?;
    let search_micros = elapsed_micros_v1(search_started);

    let mut record = TtsS1ReplayDecisionRecordV1 {
        record_ordinal,
        previous_record_sha256,
        episode_id: target.coordinates.episode_id,
        decision_ordinal: target.coordinates.decision_ordinal,
        acting_player: expected.acting_player,
        legal_action_count: expected.legal_action_count,
        chosen_action_index: full.selected_index,
        policy_sample_index: policy_sample,
        search_overrode_policy_sample: full.selected_index != policy_sample,
        requested_transitions: bound.core.authority().transition_budget,
        actual_transitions: full.transitions_used,
        simulations: full.simulations,
        tree_node_count: full.tree_node_count,
        leaf_census: full.leaf_census,
        root_statistics_digest_sha256: lower_hex_sha256_v4(root_statistics_digest_v4(&full)),
        visit_margin: visit_margin_v4(&full),
        wall_time: TtsS1DecisionWallTimeV1 {
            search_micros,
            // Filled once the record is otherwise complete, for the same
            // reason the scorer's own record fills it last.
            decision_micros: 0,
        },
        ceilings: TtsS1DecisionCeilingsV1 {
            search: classify_micros_v1(search_micros),
            decision: CeilingStatusV4::WithinSlo,
        },
    };
    let decision_micros = elapsed_micros_v1(decision_started);
    record.wall_time.decision_micros = decision_micros;
    record.ceilings.decision = classify_micros_v1(decision_micros);
    Ok(record)
}

fn elapsed_micros_v1(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Publishes the report atomically and immutably.
pub fn publish_tts_s1_replay_report_v1(
    report: &TtsS1ReplayReportV1,
    path: &Path,
) -> Result<Vec<u8>, TtsS1ReplayErrorV1> {
    let bytes = report.canonical_bytes_v1()?;
    publish_canonical_document_v1(&bytes, path)
        .map_err(|error| TtsS1ReplayErrorV1::Publication(error.to_string()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_policy_v2::FlatScoringDecisionViewV2;
    use crate::model_guided_search_authority_v1::MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1;
    use crate::native_checkpoint_inference_v1::encoded_decision_view_v1;
    use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};
    use crate::native_policy_value_net_v1::{
        NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    };
    use crate::native_tts_s1_corpus_v1::{
        corpus_body_v1, harvest_episode_v1, TtsS1CorpusSelectionV1,
    };

    /// The end-to-end fixture: the in-memory runner-fixed net that
    /// `native_checkpoint_shadow_stdio_v1`'s own search tests use
    /// (`SearchCapableTestModelV1`), with no checkpoint manifest and no
    /// Store on disk. The crate ships no Store fixture, and every
    /// real-Store scorer test is `#[ignore]`d against an external evidence
    /// root, so this is the fixture a hermetic test has; the only part of
    /// the S1 pipeline it leaves uncovered is `load_checkpoint_v1` itself,
    /// which is the scorer's own already-tested loader.
    struct RunnerFixedScorerV1 {
        net: NativePolicyValueNetV1,
    }

    impl RunnerFixedScorerV1 {
        fn new_v1() -> Self {
            Self {
                net: NativePolicyValueNetV1::runner_fixed_v1(
                    NativePolicyValueModelConfigV1::contract_v1(),
                )
                .expect("runner-fixed model builds"),
            }
        }
    }

    impl TtsS1DecisionScorerV1 for RunnerFixedScorerV1 {
        fn action_logits_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
        ) -> Result<Vec<f32>, ()> {
            let mut tensorizer = NativeFlatTensorizerV2::new();
            let mut tensor = NativeFlatDecisionTensorV2::default();
            tensorizer.fill(decision, &mut tensor).map_err(|_| ())?;
            let output = self
                .net
                .forward_v1(encoded_decision_view_v1(&tensor))
                .map_err(|_| ())?;
            if output.logits.len() != decision.actions().len() {
                return Err(());
            }
            Ok(output.logits)
        }

        fn search_net_v1(&self) -> &NativePolicyValueNetV1 {
            &self.net
        }
    }

    /// The scorer's own identity record, fabricated for the in-memory net
    /// exactly as the S0 search tests fabricate theirs. Nothing here is
    /// claimed to be a real lineage; what matters is that the SAME record
    /// feeds the corpus and the replay, so the replay's checkpoint check
    /// is exercised for real.
    fn fixture_identity_v1() -> ShadowCheckpointIdentityV1 {
        ShadowCheckpointIdentityV1 {
            authority_kind: "tts-s1-runner-fixed-test-only".to_owned(),
            source_run_sha256: "00".repeat(32),
            source_generation: 0,
            source_checkpoint_sha256: "11".repeat(32),
            source_sidecar_sha256: "22".repeat(32),
            source_payload_sha256: "33".repeat(32),
            source_train_state_sha256: "44".repeat(32),
            loaded_run_sha256: "00".repeat(32),
            loaded_generation: 0,
            loaded_checkpoint_sha256: "11".repeat(32),
            loaded_payload_sha256: "33".repeat(32),
            loaded_train_state_sha256: "44".repeat(32),
            model_parameter_sha256: "55".repeat(32),
            environment_trajectory_contract: "legacy_v1",
            sampler_identity: crate::fast_sampler::FAST_CATEGORICAL_SAMPLER_VERSION,
            sampler_contract_sha256: crate::fast_sampler::FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
        }
    }

    const FIXTURE_SEED_BLOCK_ID_V1: usize = 1;
    const FIXTURE_MAX_PHYSICAL_DECISIONS_V1: u64 = 1_024;
    const FIXTURE_MAX_POLICY_STEPS_V1: u64 = 2_048;

    /// Harvests one seeded self-play episode and seals a corpus over a
    /// strided sample of its decisions.
    ///
    /// It bypasses `select_tts_s1_corpus_v1` deliberately: the quota rules
    /// are covered exhaustively by the corpus module's own pure tests, and
    /// filling a 512-decision quota here would mean running hundreds of
    /// full searches inside a unit test. What this fixture exists to
    /// exercise is the other half, the reconstruct-verify-search-record
    /// path.
    fn fixture_corpus_v1(
        scorer: &RunnerFixedScorerV1,
        take: usize,
        stride: usize,
    ) -> TtsS1CorpusManifestV1 {
        let base_seed = MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1];
        let harvest = harvest_episode_v1(
            scorer,
            base_seed,
            0,
            FIXTURE_MAX_PHYSICAL_DECISIONS_V1,
            FIXTURE_MAX_POLICY_STEPS_V1,
        )
        .expect("the fixture episode plays");
        assert!(
            harvest.decisions.len() > take * stride,
            "the fixture episode must be long enough to sample from"
        );
        let natural = harvest.natural;
        let decisions: Vec<_> = harvest
            .into_decisions_with_action_sequences_v1()
            .into_iter()
            .step_by(stride)
            .take(take)
            .collect();
        let candidate_count = decisions.len() as u64;
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        TtsS1CorpusManifestV1::seal_v1(corpus_body_v1(
            TtsS1CorpusCheckpointV1::from_identity_v1(&fixture_identity_v1(), &architecture),
            FIXTURE_SEED_BLOCK_ID_V1,
            base_seed,
            1,
            FIXTURE_MAX_PHYSICAL_DECISIONS_V1,
            FIXTURE_MAX_POLICY_STEPS_V1,
            TtsS1CorpusSelectionV1 {
                decisions,
                candidate_count,
                natural_terminal_episode_count: u64::from(natural),
            },
        ))
        .expect("the fixture corpus seals")
    }

    fn replay_fixture_v1(
        scorer: &RunnerFixedScorerV1,
        corpus: &TtsS1CorpusManifestV1,
    ) -> TtsS1ReplayReportV1 {
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let identity = fixture_identity_v1();
        let body = replay_corpus_body_v1(
            scorer,
            &identity,
            &architecture,
            TtsS1CorpusCheckpointV1::from_identity_v1(&identity, &architecture),
            corpus,
            KernelNativeSearchTierV1::T512,
            FIXTURE_SEED_BLOCK_ID_V1,
            MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1],
            None,
        )
        .expect("the fixture replay runs");
        TtsS1ReplayReportV1::seal_v1(body).expect("the fixture report seals")
    }

    /// END TO END, t512, on a handful of decisions: self-play produces a
    /// corpus, the corpus reconstructs through the kernel, the production
    /// selector searches every reconstructed decision, and the report
    /// chains and verifies.
    #[test]
    fn a_t512_replay_runs_end_to_end_over_a_freshly_built_corpus_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 3, 7);
        let report = replay_fixture_v1(&scorer, &corpus);
        let body = &report.body;

        assert_eq!(body.tier, "t512");
        assert_eq!(body.transition_budget, 512);
        assert_eq!(body.decisions_replayed, 3);
        assert_eq!(body.corpus_decision_count, 3);
        assert!(body.replayed_whole_corpus);
        assert!(!body.stability_halves_enabled);
        assert_eq!(body.corpus_sha256, corpus.corpus_sha256);
        assert_eq!(body.slo_micros, 4_000_000);
        assert_eq!(body.hard_timeout_micros, 20_000_000);
        assert_eq!(verify_tts_s1_replay_chain_v1(body).unwrap(), 3);

        for (record, target) in body.decisions.iter().zip(corpus.body.decisions.iter()) {
            assert_eq!(record.episode_id, target.coordinates.episode_id);
            assert_eq!(record.decision_ordinal, target.coordinates.decision_ordinal);
            assert_eq!(record.legal_action_count, target.surface.legal_action_count);
            assert!(record.chosen_action_index < record.legal_action_count);
            assert!(record.policy_sample_index < record.legal_action_count);
            assert_eq!(record.requested_transitions, 512);
            assert!(record.actual_transitions >= record.legal_action_count);
            assert!(record.simulations >= 1);
            assert_eq!(record.root_statistics_digest_sha256.len(), 64);
            // The census partitions the simulations exactly, so a
            // miscounted leaf class cannot hide.
            let census = record.leaf_census;
            assert_eq!(
                u64::from(census.natural_terminal_leaves)
                    + u64::from(census.truncated_terminal_leaves)
                    + u64::from(census.newly_expanded_leaves)
                    + u64::from(census.depth_cap_leaves),
                u64::from(record.simulations)
            );
            assert!(u32::from(census.max_simulation_depth) <= 64);
        }

        // The report really is publishable and re-provable from bytes.
        let bytes = report.canonical_bytes_v1().unwrap();
        assert_eq!(decode_tts_s1_replay_report_v1(&bytes).unwrap(), report);
    }

    /// The chosen actions and the whole search product do not depend on
    /// wall time: two runs of the same tier over the same corpus strip
    /// equal. The S0 bit-identical-replay pattern, applied to S1.
    #[test]
    fn the_chosen_action_is_independent_of_timing_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 2, 11);
        let first = replay_fixture_v1(&scorer, &corpus)
            .canonical_bytes_v1()
            .unwrap();
        let second = replay_fixture_v1(&scorer, &corpus)
            .canonical_bytes_v1()
            .unwrap();
        assert_eq!(
            strip_timing_fields_v1(&first).unwrap(),
            strip_timing_fields_v1(&second).unwrap(),
            "the search product must replay exactly"
        );
        // The stripping is not a no-op on empty data: the timings really
        // are present and really do carry a measurement.
        let parsed: TtsS1ReplayReportV1 = serde_json::from_slice(&first).unwrap();
        assert!(!parsed.body.decisions.is_empty());
        assert!(
            parsed.body.decision_wall_time.max_micros >= parsed.body.search_wall_time.p50_micros
        );
    }

    /// The corpus manifest is reproducible byte for byte across two runs.
    #[test]
    fn the_corpus_manifest_is_byte_identical_across_two_runs_v1() {
        let first = fixture_corpus_v1(&RunnerFixedScorerV1::new_v1(), 4, 5)
            .canonical_bytes_v1()
            .unwrap();
        let second = fixture_corpus_v1(&RunnerFixedScorerV1::new_v1(), 4, 5)
            .canonical_bytes_v1()
            .unwrap();
        assert_eq!(first, second);
        assert!(decode_tts_s1_corpus_v1(&first).is_ok());
    }

    /// A corpus whose recorded legal surface disagrees with what the
    /// kernel actually reconstructs is refused, per surface field, before
    /// any search runs.
    #[test]
    fn a_reconstruction_mismatch_fails_closed_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 2, 9);
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let identity = fixture_identity_v1();

        /// One tampering: the surface field it corrupts, and the
        /// corruption.
        type SurfaceTamperingV1 = (&'static str, Box<dyn Fn(&mut TtsS1CorpusDecisionV1)>);
        let mutations: Vec<SurfaceTamperingV1> = vec![
            (
                "legal_action_count",
                Box::new(|decision: &mut TtsS1CorpusDecisionV1| {
                    decision.surface.legal_action_count += 1;
                }),
            ),
            (
                "core_environment_hash",
                Box::new(|decision: &mut TtsS1CorpusDecisionV1| {
                    decision.surface.core_environment_hash_u64_hex = "0".repeat(16);
                }),
            ),
            (
                "diagnostic_state_hash",
                Box::new(|decision: &mut TtsS1CorpusDecisionV1| {
                    decision.surface.diagnostic_state_hash_u64_hex = "0".repeat(16);
                }),
            ),
            (
                "physical_decision_id",
                Box::new(|decision: &mut TtsS1CorpusDecisionV1| {
                    decision.surface.physical_decision_id += 1;
                }),
            ),
            (
                "environment_seed",
                Box::new(|decision: &mut TtsS1CorpusDecisionV1| {
                    decision.coordinates.environment_seed ^= 1;
                }),
            ),
        ];
        for (field, mutate) in mutations {
            let mut tampered = corpus.clone();
            mutate(&mut tampered.body.decisions[0]);
            let tampered = TtsS1CorpusManifestV1::seal_v1(tampered.body).unwrap();
            let error = replay_corpus_body_v1(
                &scorer,
                &identity,
                &architecture,
                TtsS1CorpusCheckpointV1::from_identity_v1(&identity, &architecture),
                &tampered,
                KernelNativeSearchTierV1::T512,
                FIXTURE_SEED_BLOCK_ID_V1,
                MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1],
                None,
            )
            .expect_err("a tampered surface must fail closed");
            assert!(
                matches!(
                    error,
                    TtsS1ReplayErrorV1::ReconstructionMismatch { field: observed, .. }
                        if observed == field
                ),
                "tampering {field} produced {error}"
            );
        }
    }

    /// The replay refuses a corpus drawn from a different checkpoint.
    #[test]
    fn a_foreign_corpus_checkpoint_fails_closed_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 1, 3);
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let mut identity = fixture_identity_v1();
        identity.loaded_payload_sha256 = "ff".repeat(32);
        assert!(matches!(
            replay_corpus_body_v1(
                &scorer,
                &identity,
                &architecture,
                TtsS1CorpusCheckpointV1::from_identity_v1(&identity, &architecture),
                &corpus,
                KernelNativeSearchTierV1::T512,
                FIXTURE_SEED_BLOCK_ID_V1,
                MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1],
                None,
            ),
            Err(TtsS1ReplayErrorV1::CheckpointMismatch)
        ));
    }

    #[test]
    fn nearest_rank_percentiles_are_exact_on_a_known_set_v1() {
        // 1..=100 microseconds: rank(50) = 50 -> 50, rank(99) = 99 -> 99.
        let ascending: Vec<u64> = (1..=100).collect();
        assert_eq!(nearest_rank_percentile_micros_v1(&ascending, 50), Some(50));
        assert_eq!(nearest_rank_percentile_micros_v1(&ascending, 99), Some(99));

        // A single sample is its own p50, p99 and max.
        assert_eq!(nearest_rank_percentile_micros_v1(&[7], 50), Some(7));
        assert_eq!(nearest_rank_percentile_micros_v1(&[7], 99), Some(7));

        // Two samples: rank(50) = ceil(100/100) = 1, rank(99) = ceil(198/100) = 2.
        assert_eq!(nearest_rank_percentile_micros_v1(&[3, 9], 50), Some(3));
        assert_eq!(nearest_rank_percentile_micros_v1(&[3, 9], 99), Some(9));

        // Ten samples: rank(99) = ceil(990/100) = 10, i.e. the max.
        let ten: Vec<u64> = (1..=10).collect();
        assert_eq!(nearest_rank_percentile_micros_v1(&ten, 99), Some(10));

        assert_eq!(nearest_rank_percentile_micros_v1(&[], 50), None);
    }

    #[test]
    fn the_summary_sorts_and_totals_a_synthetic_set_v1() {
        let samples = vec![900, 100, 400, 200, 300];
        let summary = summarize_micros_v1(&samples).unwrap();
        assert_eq!(summary.p50_micros, 300);
        assert_eq!(summary.p99_micros, 900);
        assert_eq!(summary.max_micros, 900);
        assert_eq!(summary.total_micros, 1_900);
        assert!(summarize_micros_v1(&[]).is_none());
    }

    #[test]
    fn the_ceilings_are_the_pre_registered_constants_v1() {
        assert_eq!(slo_micros_v1(), 4_000_000);
        assert_eq!(hard_timeout_micros_v1(), 20_000_000);
        assert_eq!(classify_micros_v1(0), CeilingStatusV4::WithinSlo);
        assert_eq!(classify_micros_v1(4_000_000), CeilingStatusV4::WithinSlo);
        assert_eq!(classify_micros_v1(4_000_001), CeilingStatusV4::SloExceeded);
        assert_eq!(classify_micros_v1(19_999_999), CeilingStatusV4::SloExceeded);
        assert_eq!(
            classify_micros_v1(20_000_000),
            CeilingStatusV4::HardTimeoutExceeded
        );
    }

    #[test]
    fn a_tier_over_the_slo_is_reported_infeasible_v1() {
        let (verdict, reason) = verdict_v1(
            CeilingStatusV4::SloExceeded,
            CeilingStatusV4::SloExceeded,
            true,
        );
        assert_eq!(verdict, TtsS1TierVerdictV1::Infeasible);
        assert!(reason.contains("exceeds"));

        let (verdict, _) = verdict_v1(
            CeilingStatusV4::WithinSlo,
            CeilingStatusV4::HardTimeoutExceeded,
            true,
        );
        assert_eq!(
            verdict,
            TtsS1TierVerdictV1::Infeasible,
            "a decision that reached the hard timeout is a product failure"
        );

        let (verdict, _) = verdict_v1(CeilingStatusV4::WithinSlo, CeilingStatusV4::WithinSlo, true);
        assert_eq!(verdict, TtsS1TierVerdictV1::Feasible);

        let (verdict, reason) = verdict_v1(
            CeilingStatusV4::WithinSlo,
            CeilingStatusV4::WithinSlo,
            false,
        );
        assert_eq!(
            verdict,
            TtsS1TierVerdictV1::Infeasible,
            "a partial replay never carries a feasibility verdict"
        );
        assert!(reason.contains("partial"));
    }

    fn synthetic_record_v1(ordinal: u64, previous: String) -> TtsS1ReplayDecisionRecordV1 {
        TtsS1ReplayDecisionRecordV1 {
            record_ordinal: ordinal,
            previous_record_sha256: previous,
            episode_id: 0,
            decision_ordinal: ordinal,
            acting_player: PlayerSeatV1::P0,
            legal_action_count: 4,
            chosen_action_index: 1,
            policy_sample_index: 2,
            search_overrode_policy_sample: true,
            requested_transitions: 512,
            actual_transitions: 512,
            simulations: 128,
            tree_node_count: 40,
            leaf_census: ModelGuidedSearchLeafCensusV1 {
                natural_terminal_leaves: 1,
                truncated_terminal_leaves: 0,
                newly_expanded_leaves: 100,
                depth_cap_leaves: 27,
                max_simulation_depth: 9,
                summed_simulation_depth: 512,
            },
            root_statistics_digest_sha256: "ab".repeat(32),
            visit_margin: 7,
            wall_time: TtsS1DecisionWallTimeV1 {
                search_micros: 1_000 + ordinal,
                decision_micros: 2_000 + ordinal,
            },
            ceilings: TtsS1DecisionCeilingsV1 {
                search: CeilingStatusV4::WithinSlo,
                decision: CeilingStatusV4::WithinSlo,
            },
        }
    }

    fn synthetic_body_v1(record_count: u64) -> TtsS1ReplayReportBodyV1 {
        let mut previous = TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned();
        let mut records = Vec::new();
        for ordinal in 0..record_count {
            let record = synthetic_record_v1(ordinal, previous.clone());
            previous = lower_hex_sha256_v4(record.chain_link_v1().unwrap());
            records.push(record);
        }
        let search: Vec<u64> = records
            .iter()
            .map(|record| record.wall_time.search_micros)
            .collect();
        let decision: Vec<u64> = records
            .iter()
            .map(|record| record.wall_time.decision_micros)
            .collect();
        TtsS1ReplayReportBodyV1 {
            engine_commit: "deadbeef".to_owned(),
            tier: "t512".to_owned(),
            transition_budget: 512,
            policy_step_depth_cap: 64,
            seed_block_id: 1,
            seed_block_seed: 3_102_001,
            stability_halves_enabled: false,
            checkpoint: TtsS1CorpusCheckpointV1 {
                authority_kind: "test-only".to_owned(),
                loaded_run_sha256: "00".repeat(32),
                loaded_generation: 0,
                loaded_checkpoint_sha256: "11".repeat(32),
                loaded_payload_sha256: "22".repeat(32),
                loaded_train_state_sha256: "33".repeat(32),
                model_parameter_sha256: "44".repeat(32),
                net_architecture_identity: "kernel-policy-value-net-8".to_owned(),
                environment_trajectory_contract: "legacy_v1".to_owned(),
                sampler_identity: "sampler".to_owned(),
                sampler_contract_sha256: "55".repeat(32),
            },
            wrapper_identity: WrapperIdentityV4 {
                core_algorithm_identity: "core".to_owned(),
                authority_kind: "kind".to_owned(),
                authority_schema: "schema".to_owned(),
                node_key_identity: "node".to_owned(),
                seed_domain: "domain".to_owned(),
                tier: "t512".to_owned(),
                transition_budget: 512,
                policy_step_depth_cap: 64,
                seed_block_id: 1,
                action_seed_u64_hex: "00000000002f5bf1".to_owned(),
                search_authority_digest_sha256: "66".repeat(32),
                checkpoint_lineage_id: "lineage".to_owned(),
                net_architecture_identity: "kernel-policy-value-net-8".to_owned(),
                puct_prior_quantization_contract_sha256: "77".repeat(32),
                value_quantization_contract_sha256: "88".repeat(32),
                forward_determinism_build_identity: "99".repeat(32),
                value_head_domain: "calibrated".to_owned(),
                checkpoint_manifest_sha256: "11".repeat(32),
                checkpoint_model_parameter_sha256: "44".repeat(32),
                engine_commit: "deadbeef".to_owned(),
            },
            search_authority_digest_sha256: "66".repeat(32),
            corpus_sha256: "aa".repeat(32),
            corpus_decision_count: record_count,
            decisions_replayed: record_count,
            replayed_whole_corpus: true,
            percentile_rule: TTS_S1_PERCENTILE_RULE_V1.to_owned(),
            search_wall_time: summarize_micros_v1(&search).unwrap(),
            decision_wall_time: summarize_micros_v1(&decision).unwrap(),
            decisions_per_second_milli: 1,
            slo_micros: slo_micros_v1(),
            hard_timeout_micros: hard_timeout_micros_v1(),
            p99_decision_ceiling_status: CeilingStatusV4::WithinSlo,
            max_decision_ceiling_status: CeilingStatusV4::WithinSlo,
            verdict: TtsS1TierVerdictV1::Feasible,
            verdict_reason: "synthetic".to_owned(),
            chain_genesis_sha256: TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned(),
            final_record_sha256: previous,
            decisions: records,
        }
    }

    #[test]
    fn the_report_round_trips_and_its_chain_verifies_v1() {
        let body = synthetic_body_v1(5);
        assert_eq!(verify_tts_s1_replay_chain_v1(&body).unwrap(), 5);
        let report = TtsS1ReplayReportV1::seal_v1(body).unwrap();
        let bytes = report.canonical_bytes_v1().unwrap();
        assert_eq!(decode_tts_s1_replay_report_v1(&bytes).unwrap(), report);
    }

    #[test]
    fn a_tampered_decision_breaks_the_chain_v1() {
        let mut body = synthetic_body_v1(4);
        body.decisions[1].chosen_action_index += 1;
        assert!(matches!(
            verify_tts_s1_replay_chain_v1(&body),
            Err(TtsS1ReplayErrorV1::BrokenChain)
        ));
    }

    #[test]
    fn stripping_the_timings_leaves_the_substantive_claim_v1() {
        // Two "runs" that differ only in their timings must strip equal.
        let mut slow = synthetic_body_v1(3);
        for (index, record) in slow.decisions.iter_mut().enumerate() {
            record.wall_time.search_micros = 5_000_000 + index as u64;
            record.wall_time.decision_micros = 6_000_000 + index as u64;
            record.ceilings.search = classify_micros_v1(record.wall_time.search_micros);
            record.ceilings.decision = classify_micros_v1(record.wall_time.decision_micros);
        }
        // Rebuild the chain over the changed records, as a real run would.
        let mut previous = TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned();
        for record in slow.decisions.iter_mut() {
            record.previous_record_sha256 = previous.clone();
            previous = lower_hex_sha256_v4(record.chain_link_v1().unwrap());
        }
        slow.final_record_sha256 = previous;
        let search: Vec<u64> = slow
            .decisions
            .iter()
            .map(|record| record.wall_time.search_micros)
            .collect();
        let decision: Vec<u64> = slow
            .decisions
            .iter()
            .map(|record| record.wall_time.decision_micros)
            .collect();
        slow.search_wall_time = summarize_micros_v1(&search).unwrap();
        slow.decision_wall_time = summarize_micros_v1(&decision).unwrap();
        slow.p99_decision_ceiling_status = classify_micros_v1(slow.decision_wall_time.p99_micros);
        slow.max_decision_ceiling_status = classify_micros_v1(slow.decision_wall_time.max_micros);
        let (verdict, reason) = verdict_v1(
            slow.p99_decision_ceiling_status,
            slow.max_decision_ceiling_status,
            true,
        );
        slow.verdict = verdict;
        slow.verdict_reason = reason;

        let fast_bytes = TtsS1ReplayReportV1::seal_v1(synthetic_body_v1(3))
            .unwrap()
            .canonical_bytes_v1()
            .unwrap();
        let slow_bytes = TtsS1ReplayReportV1::seal_v1(slow)
            .unwrap()
            .canonical_bytes_v1()
            .unwrap();
        assert_ne!(fast_bytes, slow_bytes, "the two runs really do differ");
        assert_eq!(
            strip_timing_fields_v1(&fast_bytes).unwrap(),
            strip_timing_fields_v1(&slow_bytes).unwrap(),
            "the chosen actions and search products must not depend on timing"
        );
    }

    #[test]
    fn the_tier_ladder_is_strict_v1() {
        assert_eq!(
            parse_tts_s1_tier_v1("t512"),
            Some(KernelNativeSearchTierV1::T512)
        );
        assert_eq!(
            parse_tts_s1_tier_v1("t32768"),
            Some(KernelNativeSearchTierV1::T32768)
        );
        for bad in ["512", "T512", "t1024", "t512 ", ""] {
            assert!(
                parse_tts_s1_tier_v1(bad).is_none(),
                "{bad:?} must be rejected"
            );
        }
    }
}
