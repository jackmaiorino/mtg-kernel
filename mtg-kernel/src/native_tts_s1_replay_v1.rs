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
    episode_decision_ceilings_v4, lower_hex_sha256_v4, root_statistics_digest_v4, visit_margin_v4,
    CeilingStatusV4, EpisodeCloseReasonV4, ModelGuidedSearchOutcomeWriterV4, ProtocolRequestKindV4,
    SearchDecisionRecordV4, WallTimeV4, WrapperIdentityV4,
    MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4,
    MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4, MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V4,
};
use crate::native_checkpoint_shadow_stdio_v1::{
    decision_kind_v1, load_checkpoint_v1, model_guided_search_full_budget_v1,
    model_guided_search_pinned_evaluator_v1, model_input_sha256_v1, BoundModelGuidedSearchV1,
    ShadowCheckpointAuthorityV1, ShadowCheckpointIdentityV1,
    CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1, CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1,
    CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V1, SHADOW_RANDOMIZATION_IDENTITY_V1,
};
use crate::native_trainer_schedule_v1::{
    native_trainer_episode_schedule_v1, NativeTrainerEpisodeScheduleV1,
};
use crate::native_tts_s1_corpus_v1::{
    corpus_policy_sample_seed_v1, decode_tts_s1_corpus_v1, nearest_rank_percentile_v1,
    publish_canonical_document_v1, TtsS1CorpusCheckpointV1, TtsS1CorpusDecisionV1,
    TtsS1CorpusErrorV1, TtsS1CorpusManifestV1, TtsS1DecisionScorerV1,
    TTS_S1_NEAREST_RANK_PERCENTILE_RULE_V1,
};
use crate::rl::{ActionSemanticV1, PlayerSeatV1};
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, CANONICAL_RALLY_DECK_ID,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Wire schema of the per-tier feasibility report.
pub const TTS_S1_REPLAY_REPORT_SCHEMA_V1: &str = "mtg-kernel-tts-s1-replay-report/v1";

/// How the two percentiles are defined, stated on the wire so nobody has
/// to guess which of the several common conventions produced them.
pub const TTS_S1_PERCENTILE_RULE_V1: &str = TTS_S1_NEAREST_RANK_PERCENTILE_RULE_V1;

/// Sketch Section 4: "N = 3,072 root clusters (6,144 paired units) per
/// tier, EQUAL across tiers".
pub const TTS_S1_S2_ROOT_CLUSTERS_V1: u64 = 3_072;
/// "two seat-swapped paired units form one root cluster".
pub const TTS_S1_S2_PAIRED_UNITS_PER_ROOT_CLUSTER_V1: u64 = 2;
/// Sketch Section 4: "the 16-worker host".
pub const TTS_S1_S2_WORKERS_V1: u64 = 16;
/// Sketch Section 4: "a tier whose projected S2 cost (from S1 timings)
/// exceeds 48 worker-hours on the 16-worker host is INFEASIBLE and dropped
/// before any S2 game". Scaled by 1,000 so the whole projection stays in
/// integers, which the canonical JSON codec requires.
pub const TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1: u64 = 48_000;

/// The projection, spelled out on the wire.
pub const TTS_S1_S2_PROJECTION_RULE_V1: &str = concat!(
    "wrapped-games-only",
    "-3072-root-clusters-times-2-paired-units",
    "-times-mean-decisions-per-natural-terminal-episode",
    "-times-mean-protocol-decision-wall-time",
    "-over-16-workers",
    "/v1"
);

/// Why the raw-policy co-measurement is not in the projection.
pub const TTS_S1_RAW_POLICY_EXCLUSION_REASON_V1: &str = concat!(
    "the raw-policy arm of each paired unit is one flat forward and one ",
    "temperature-1 sample per decision, orders of magnitude below a tiered ",
    "search on the same decision, so its contribution to the 48 worker-hour ",
    "cap is negligible and is excluded; the projection covers the wrapped ",
    "games only"
);

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
    /// DIAGNOSTIC.
    pub search_micros: u64,
    /// The scorer's own `decision_micros` phase: the flat encode, the
    /// tensorization, the policy forward, the policy sample, the search,
    /// and the V4 record's construction, stopping where the record is
    /// handed to the diagnostics writer. DIAGNOSTIC, and by construction
    /// an UNDER-count of what a client waits for, which is why it is not
    /// what the SLO is classified on.
    ///
    /// It deliberately excludes the fast-forward that repositioned the
    /// session, which is an artifact of replaying a corpus and is not work
    /// any live decision does.
    pub decision_micros: u64,
    /// The production diagnostics writer's own measurement of publishing
    /// THIS decision's V4 record: serialization, the chain hash, the
    /// episode rebuild and the durable move. Read back off the successor
    /// record through `episode_decision_ceilings_v4`, never computed here.
    pub publish_micros: u64,
    /// The response tail measured from publication completion: the
    /// scorer-shaped response line's serialization, write and flush. Also
    /// writer-observed and read back off the successor record.
    pub response_micros: u64,
    /// `decision + publish + response`: the whole synchronous cost a
    /// client waits for, and the ONLY timing the SLO verdict is
    /// classified on.
    pub protocol_micros: u64,
}

/// Where each timing landed against the pre-registered ceilings.
/// Recorded, never acted on.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1DecisionCeilingsV1 {
    pub search: CeilingStatusV4,
    pub decision: CeilingStatusV4,
    /// The verdict-bearing one.
    pub protocol: CeilingStatusV4,
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

/// One published V4 diagnostics episode file, so the report commits to the
/// artifacts its protocol latencies were read from.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1DiagnosticsEpisodeFileV1 {
    pub episode_id: u64,
    pub file_name: String,
    pub bytes: u64,
    pub sha256: String,
    pub decision_record_count: u64,
}

/// Sketch Section 4's compute cap, with every input to the projection.
///
/// The rule, fixed here and restated on the wire:
///
/// ```text
/// wrapped_games   = 3,072 root clusters x 2 paired units = 6,144
/// worker_seconds  = wrapped_games
///                 x mean decisions per natural-terminal episode
///                 x mean PROTOCOL decision wall time
///                 / 16 workers
/// ```
///
/// Two deliberate choices, both conservative and both recorded:
///
/// The mean decisions per game comes from the CORPUS, which is the only
/// part of S1 that ever played whole games, and counts BOTH seats'
/// decisions. An S2 wrapped agent occupies one seat, so the true wrapped
/// decision count per game is smaller and the projection over-estimates.
///
/// The mean wall time is the PROTOCOL latency, not the narrower
/// `decision_micros`, because a worker's hour is spent on everything the
/// client waits for, publication and response included. Both means are on
/// the wire, so an auditor can recompute on either basis.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ComputeCapProjectionV1 {
    pub rule: String,
    pub s2_root_clusters: u64,
    pub s2_paired_units_per_root_cluster: u64,
    pub s2_wrapped_games: u64,
    pub s2_workers: u64,
    /// From the corpus: mean decisions per natural-terminal episode times
    /// 1,000, floored.
    pub mean_decisions_per_game_milli: u64,
    /// Corpus context for the mean above.
    pub p99_decisions_per_game: u64,
    pub max_decisions_per_game: u64,
    pub natural_terminal_episode_count: u64,
    /// The mean this projection multiplies by.
    pub mean_protocol_micros: u64,
    /// Recorded beside it so the projection can be recomputed on the
    /// narrower basis too.
    pub mean_decision_micros: u64,
    pub mean_search_micros: u64,
    pub raw_policy_games_excluded: bool,
    pub raw_policy_exclusion_reason: String,
    pub projected_worker_hours_milli: u64,
    pub cap_worker_hours_milli: u64,
    pub within_cap: bool,
}

/// The projection itself. Integer arithmetic in `u128` throughout, so a
/// slow tier cannot overflow its own cost estimate into a small number.
pub fn project_s2_worker_hours_milli_v1(
    mean_decisions_per_game_milli: u64,
    mean_protocol_micros: u64,
) -> u64 {
    let wrapped_games = u128::from(TTS_S1_S2_ROOT_CLUSTERS_V1)
        .saturating_mul(u128::from(TTS_S1_S2_PAIRED_UNITS_PER_ROOT_CLUSTER_V1));
    let total_micros = wrapped_games
        .saturating_mul(u128::from(mean_decisions_per_game_milli))
        .saturating_mul(u128::from(mean_protocol_micros))
        / 1_000;
    let per_worker_micros = total_micros / u128::from(TTS_S1_S2_WORKERS_V1);
    // micros -> worker-hours, times 1,000: divide by 3.6e9, multiply by 1e3.
    let milli = per_worker_micros.saturating_mul(1_000) / 3_600_000_000u128;
    u64::try_from(milli).unwrap_or(u64::MAX)
}

/// Builds the projection block for one tier.
pub fn compute_cap_projection_v1(
    episode_decisions: &crate::native_tts_s1_corpus_v1::TtsS1EpisodeDecisionStatsV1,
    mean_protocol_micros: u64,
    mean_decision_micros: u64,
    mean_search_micros: u64,
) -> TtsS1ComputeCapProjectionV1 {
    let projected_worker_hours_milli = project_s2_worker_hours_milli_v1(
        episode_decisions.mean_decisions_milli,
        mean_protocol_micros,
    );
    TtsS1ComputeCapProjectionV1 {
        rule: TTS_S1_S2_PROJECTION_RULE_V1.to_owned(),
        s2_root_clusters: TTS_S1_S2_ROOT_CLUSTERS_V1,
        s2_paired_units_per_root_cluster: TTS_S1_S2_PAIRED_UNITS_PER_ROOT_CLUSTER_V1,
        s2_wrapped_games: TTS_S1_S2_ROOT_CLUSTERS_V1
            .saturating_mul(TTS_S1_S2_PAIRED_UNITS_PER_ROOT_CLUSTER_V1),
        s2_workers: TTS_S1_S2_WORKERS_V1,
        mean_decisions_per_game_milli: episode_decisions.mean_decisions_milli,
        p99_decisions_per_game: episode_decisions.p99_decisions,
        max_decisions_per_game: episode_decisions.max_decisions,
        natural_terminal_episode_count: episode_decisions.natural_terminal_episode_count,
        mean_protocol_micros,
        mean_decision_micros,
        mean_search_micros,
        raw_policy_games_excluded: true,
        raw_policy_exclusion_reason: TTS_S1_RAW_POLICY_EXCLUSION_REASON_V1.to_owned(),
        projected_worker_hours_milli,
        cap_worker_hours_milli: TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1,
        within_cap: projected_worker_hours_milli <= TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1,
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
    nearest_rank_percentile_v1(samples, percentile)
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
    /// DIAGNOSTIC: the search alone.
    pub search_wall_time: TtsS1TimingSummaryV1,
    /// DIAGNOSTIC: the scorer's `decision_micros` phase alone.
    pub decision_wall_time: TtsS1TimingSummaryV1,
    /// THE VERDICT BASIS: decision plus the production diagnostics
    /// writer's own publication measurement plus the measured response
    /// tail, exactly as `episode_decision_ceilings_v4` reconstructs a
    /// panel's per-decision latency from a published episode file.
    pub protocol_wall_time: TtsS1TimingSummaryV1,
    /// Throughput as decisions per second times 1,000, floored, over the
    /// PROTOCOL total. Scaled to an integer because the canonical JSON
    /// codec forbids floats outright; the operands are on the wire beside
    /// it, so the exact rational is recoverable.
    pub decisions_per_second_milli: u64,
    pub slo_micros: u64,
    pub hard_timeout_micros: u64,
    pub p99_protocol_ceiling_status: CeilingStatusV4,
    pub max_protocol_ceiling_status: CeilingStatusV4,
    /// The published V4 diagnostics episode files the protocol latencies
    /// were read from.
    pub diagnostics_episode_files: Vec<TtsS1DiagnosticsEpisodeFileV1>,
    /// Sketch Section 4's 48 worker-hour compute cap, and every input.
    pub compute_cap: TtsS1ComputeCapProjectionV1,
    pub verdict: TtsS1TierVerdictV1,
    /// Every failing clause, or the sentence that says none failed.
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
        "protocol_wall_time",
        "decisions_per_second_milli",
        "p99_protocol_ceiling_status",
        "max_protocol_ceiling_status",
        "diagnostics_episode_files",
        "compute_cap",
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
    /// The production diagnostics writer, the response sink, or the
    /// published episode file it all has to be read back from.
    Diagnostics(String),
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
            Self::Diagnostics(_) => "tts_s1_replay_diagnostics_failed",
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
            | Self::Diagnostics(detail)
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
    /// Where the PRODUCTION model-guided diagnostics writer publishes this
    /// run's V4 episode files, and where the scorer-shaped response lines
    /// are written. Not optional: the protocol latency the SLO is
    /// classified on is measured BY that writer, so a run without it could
    /// only report the narrower `decision_micros` and would understate
    /// what a panel pays.
    pub diagnostics_directory: PathBuf,
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
        &config.diagnostics_directory,
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
    diagnostics_directory: &Path,
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

    // THE PRODUCTION DIAGNOSTICS WRITER. Not a stand-in: this is the same
    // `ModelGuidedSearchOutcomeWriterV4` a CP7 panel's scorer runs, so the
    // publication cost it measures (serialization, the chain hash, the
    // episode rebuild that grows with the episode, and the durable move)
    // is the cost a panel actually pays, including its growth over a long
    // game. Nothing here recomputes those phases; they are read back off
    // the published files through `episode_decision_ceilings_v4`, which is
    // the only correct way to classify a decision's protocol ceiling.
    std::fs::create_dir_all(diagnostics_directory)
        .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
    let mut diagnostics =
        ModelGuidedSearchOutcomeWriterV4::open_directory_v4(diagnostics_directory.to_path_buf())
            .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
    // The response sink stands in for the scorer's stdout: one stream for
    // the whole run, one `writeln!` and one `flush` per decision, exactly
    // as `run_checkpoint_shadow_stdio_*`'s serving loop does.
    let responses_path = diagnostics_directory.join(TTS_S1_RESPONSE_LINES_FILE_V1);
    let mut responses = std::io::BufWriter::new(
        std::fs::File::create(&responses_path)
            .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?,
    );

    // (episode id, published path, the record indices written for it, in
    // write order) so the writer-observed phases can be matched back.
    let mut published_episodes: Vec<(u64, PathBuf, Vec<usize>)> = Vec::new();

    let mut cursor = 0usize;
    let targets = &corpus.body.decisions;
    while cursor < targets.len() && (records.len() as u64) < planned {
        let episode_id = targets[cursor].coordinates.episode_id;
        let (mut session, schedule) = reset_episode_v1(
            &targets[cursor],
            corpus.body.max_physical_decisions,
            corpus.body.max_policy_steps,
        )?;
        let mut applied: Vec<u32> = Vec::new();
        let mut episode_record_indices: Vec<usize> = Vec::new();
        let mut episode_opened = false;

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

            if !episode_opened {
                // The header opens BEFORE the first searched decision, as
                // the scorer's own reset path does, so the first decision
                // record has something to chain to.
                diagnostics
                    .begin_episode_v4(
                        episode_id,
                        corpus.body.seed_block_seed,
                        schedule.learner_seat,
                        bound_ref.wrapper_identity.clone(),
                    )
                    .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
                episode_opened = true;
            }

            let context = TtsS1DecisionContextV1 {
                identity,
                deck_ids: &corpus.body.deck_ids,
                base_seed: corpus.body.seed_block_seed,
                schedule: &schedule,
            };
            let record = search_and_publish_one_decision_v1(
                scorer,
                bound_ref,
                &value_domain,
                &session,
                expected,
                target,
                &context,
                &mut diagnostics,
                &mut responses,
            )?;
            episode_record_indices.push(records.len());
            records.push(record);
            cursor += 1;
        }

        if episode_opened {
            let path = diagnostics.episode_path_v4(episode_id, corpus.body.seed_block_seed);
            // An S1 replay never plays an episode to a terminal: it visits
            // a stratified sample of one and moves on. Closing with the
            // reason that is actually true (this episode was replaced by
            // the next, or the run ended) keeps the footer honest, and the
            // footer is what gives the episode's LAST decision a successor
            // and therefore a protocol verdict at all.
            let more_to_come = cursor < targets.len() && (records.len() as u64) < planned;
            let reason = if more_to_come {
                EpisodeCloseReasonV4::EpisodeReplaced
            } else {
                EpisodeCloseReasonV4::ProcessExit
            };
            diagnostics
                .close_episode_v4(reason)
                .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
            published_episodes.push((episode_id, path, episode_record_indices));
        }

        // The inner loop leaves `cursor` either at the first decision of
        // the next episode (this episode is finished) or at an unreplayed
        // decision of this one (the smoke budget ran out). The outer
        // loop's own budget test covers the second case, so nothing more
        // is needed here to terminate.
    }

    responses
        .flush()
        .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
    drop(responses);

    let decisions_replayed = records.len() as u64;
    if decisions_replayed == 0 {
        return Err(TtsS1ReplayErrorV1::NoDecisions);
    }

    // Read the writer-observed phases back off the PUBLISHED files. Every
    // episode here is closed with a footer, so every decision has a
    // successor and therefore a protocol latency; a `None` would mean the
    // file was not closed and is a fail-closed error rather than a
    // silently dropped sample.
    let mut diagnostics_episode_files = Vec::new();
    for (episode_id, path, indices) in &published_episodes {
        let bytes = std::fs::read(path)
            .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
        let ceilings =
            episode_decision_ceilings_v4(&bytes).map_err(TtsS1ReplayErrorV1::Diagnostics)?;
        if ceilings.len() != indices.len() {
            return Err(TtsS1ReplayErrorV1::Diagnostics(format!(
                "episode {episode_id} published {} decision records for {} replayed decisions",
                ceilings.len(),
                indices.len()
            )));
        }
        for (ceiling, index) in ceilings.iter().zip(indices.iter().copied()) {
            let (Some(publish), Some(response), Some(protocol), Some(status)) = (
                ceiling.publish_micros,
                ceiling.response_micros,
                ceiling.protocol_micros,
                ceiling.protocol_ceiling_status,
            ) else {
                return Err(TtsS1ReplayErrorV1::Diagnostics(format!(
                    "episode {episode_id} decision {} has no successor record",
                    ceiling.decision_ordinal
                )));
            };
            let record = &mut records[index];
            if record.wall_time.decision_micros != ceiling.decision_micros {
                return Err(TtsS1ReplayErrorV1::Diagnostics(format!(
                    "episode {episode_id} decision {} does not match the published record",
                    ceiling.decision_ordinal
                )));
            }
            record.wall_time.publish_micros = publish;
            record.wall_time.response_micros = response;
            record.wall_time.protocol_micros = protocol;
            record.ceilings.protocol = status;
        }
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest: [u8; 32] = hasher.finalize().into();
        diagnostics_episode_files.push(TtsS1DiagnosticsEpisodeFileV1 {
            episode_id: *episode_id,
            file_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes: bytes.len() as u64,
            sha256: lower_hex_sha256_v4(digest),
            decision_record_count: ceilings.len() as u64,
        });
    }

    // The chain is assigned LAST, over finished records, because the
    // writer-observed phases above are part of what it commits to.
    let mut previous_record_sha256 = TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned();
    for (ordinal, record) in records.iter_mut().enumerate() {
        record.record_ordinal = ordinal as u64;
        record.previous_record_sha256 = previous_record_sha256.clone();
        previous_record_sha256 = lower_hex_sha256_v4(record.chain_link_v1()?);
    }

    let search_samples: Vec<u64> = records
        .iter()
        .map(|record| record.wall_time.search_micros)
        .collect();
    let decision_samples: Vec<u64> = records
        .iter()
        .map(|record| record.wall_time.decision_micros)
        .collect();
    let protocol_samples: Vec<u64> = records
        .iter()
        .map(|record| record.wall_time.protocol_micros)
        .collect();
    let search_wall_time =
        summarize_micros_v1(&search_samples).ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
    let decision_wall_time =
        summarize_micros_v1(&decision_samples).ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
    let protocol_wall_time =
        summarize_micros_v1(&protocol_samples).ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
    let decisions_per_second_milli = if protocol_wall_time.total_micros == 0 {
        0
    } else {
        decisions_replayed
            .saturating_mul(MICROS_PER_SECOND_V1)
            .saturating_mul(1_000)
            / protocol_wall_time.total_micros
    };
    let compute_cap = compute_cap_projection_v1(
        &corpus.body.episode_decisions,
        protocol_wall_time.total_micros / decisions_replayed,
        decision_wall_time.total_micros / decisions_replayed,
        search_wall_time.total_micros / decisions_replayed,
    );
    let p99_protocol_ceiling_status = classify_micros_v1(protocol_wall_time.p99_micros);
    let max_protocol_ceiling_status = classify_micros_v1(protocol_wall_time.max_micros);
    let replayed_whole_corpus = decisions_replayed == corpus_decision_count;
    let (verdict, verdict_reason) = verdict_v1(
        p99_protocol_ceiling_status,
        max_protocol_ceiling_status,
        compute_cap.within_cap,
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
        protocol_wall_time,
        decisions_per_second_milli,
        slo_micros: slo_micros_v1(),
        hard_timeout_micros: hard_timeout_micros_v1(),
        p99_protocol_ceiling_status,
        max_protocol_ceiling_status,
        diagnostics_episode_files,
        compute_cap,
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
/// Four independent INFEASIBLE clauses, each from the sketch, and EVERY
/// failing one is named rather than only the first: a tier that both blows
/// the SLO and blows the compute cap should say so, because dropping one
/// clause would be re-measured for nothing after the other was fixed.
///
/// 1. The run did not cover the whole frozen corpus. A smoke has no
///    feasibility standing at all.
/// 2. p99 PROTOCOL wall time is not inside the 4.0 s SLO (Section 4).
/// 3. The slowest decision reached the 20.0 s hard protocol timeout, which
///    inside a formal panel is a product failure of that panel, so a tier
///    that can produce one is not admissible.
/// 4. The projected S2 cost exceeds the 48 worker-hour compute cap
///    (Section 4: "INFEASIBLE and dropped before any S2 game").
///
/// Every branch produces a report; none of them is silent.
pub fn verdict_v1(
    p99: CeilingStatusV4,
    max: CeilingStatusV4,
    within_compute_cap: bool,
    replayed_whole_corpus: bool,
) -> (TtsS1TierVerdictV1, String) {
    let mut failures: Vec<String> = Vec::new();
    if !replayed_whole_corpus {
        failures.push(
            "partial replay: a feasibility verdict requires the whole frozen corpus".to_owned(),
        );
    }
    if p99 != CeilingStatusV4::WithinSlo {
        failures.push(format!(
            "p99 protocol wall time exceeds the {MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4} s SLO"
        ));
    }
    if max == CeilingStatusV4::HardTimeoutExceeded {
        failures.push(format!(
            "the slowest decision reached the {MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4} s hard protocol timeout"
        ));
    }
    if !within_compute_cap {
        failures.push(format!(
            "the projected S2 cost exceeds the {} worker-hour compute cap",
            TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1 / 1_000
        ));
    }
    if failures.is_empty() {
        return (
            TtsS1TierVerdictV1::Feasible,
            format!(
                "the whole corpus replayed; p99 protocol wall time is inside the {MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4} s SLO, no decision reached the {MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4} s hard protocol timeout, and the projected S2 cost is inside the {} worker-hour compute cap",
                TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1 / 1_000
            ),
        );
    }
    (TtsS1TierVerdictV1::Infeasible, failures.join("; "))
}

fn reset_episode_v1(
    target: &TtsS1CorpusDecisionV1,
    max_physical_decisions: u64,
    max_policy_steps: u64,
) -> Result<(FastActorSessionV1, NativeTrainerEpisodeScheduleV1), TtsS1ReplayErrorV1> {
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
    let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
        coordinates.episode_id,
        schedule.environment_seed,
        max_physical_decisions,
        max_policy_steps,
        [
            CANONICAL_RALLY_DECK_ID.to_owned(),
            CANONICAL_RALLY_DECK_ID.to_owned(),
        ],
    )
    .map_err(|error| TtsS1ReplayErrorV1::SessionReset(format!("{:?}", error.code)))?;
    Ok((session, schedule))
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
    // The decision KIND is part of the legal surface, not decoration: a
    // surface decision and an attacker-inclusion decision at the same
    // width are different questions, and searching the wrong one would be
    // a real measurement of the wrong thing. Compared through the scorer's
    // own wire spelling, which is the same function the corpus recorded
    // with.
    if decision_kind_v1(expected.decision_kind) != recorded.decision_kind {
        return Err(mismatch("decision_kind"));
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

/// The file the scorer-shaped response lines are written to. It stands in
/// for the scorer's stdout: one stream, one line per decision, flushed.
pub const TTS_S1_RESPONSE_LINES_FILE_V1: &str = "scorer-shaped-responses.jsonl";

/// The per-episode facts a scorer-shaped response line needs and the
/// decision itself does not carry.
struct TtsS1DecisionContextV1<'a> {
    identity: &'a ShadowCheckpointIdentityV1,
    deck_ids: &'a [String; 2],
    base_seed: u64,
    schedule: &'a NativeTrainerEpisodeScheduleV1,
}

/// A scorer-shaped `decision` response body.
///
/// Field for field the payload-bearing half of the CP7 scorer's own
/// `DecisionBodyV1`: the same envelope, the same decision identity, the
/// same commitments and hashes, and above all the same three
/// width-proportional payloads (the per-action logit bit vector, the value
/// bits, and the ordered action semantics), which are what make the
/// serialization and the write cost what they are. The point of this type
/// is the SIZE and SHAPE of the line, because the response tail is the
/// phase it is measured in.
///
/// It is deliberately a separate type rather than the scorer's own: that
/// one is reachable only through `ActiveShadowSessionV1` and a live stdio
/// service, and standing one of those up would put a protocol driver in
/// the middle of a feasibility measurement. `initial_library_card_definition_ids`
/// is absent because every S1 window is a STEP window (the replay never
/// pays a reset's one-off session construction), which is also why the
/// V4 records below carry `ProtocolRequestKindV4::Step`.
#[derive(Serialize)]
struct TtsS1ScorerShapedResponseV1<'a> {
    protocol: &'static str,
    schema_version: u32,
    request_id: String,
    checkpoint: &'a ShadowCheckpointIdentityV1,
    kind: &'static str,
    deck_ids: &'a [String; 2],
    randomization_identity: &'static str,
    base_seed_u64_hex: String,
    pair_index: u64,
    pair_environment_seed_u64_hex: String,
    episode_id: u64,
    step: u64,
    environment_revision: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    acting_player: PlayerSeatV1,
    decision_kind: &'static str,
    legal_action_count: u32,
    candidate_seat: PlayerSeatV1,
    candidate_controls_current_actor: bool,
    candidate_action_seed_u64_hex: String,
    selected_action_index: u32,
    candidate_order_commitment_128_hex: String,
    model_input_commitment: &'static str,
    model_input_sha256: String,
    diagnostic_state_hash_u64_hex: String,
    core_environment_hash_u64_hex: String,
    logits_f32_bits: Vec<u32>,
    value_f32_bits: u32,
    action_semantics: Vec<ActionSemanticV1>,
}

/// Searches one reconstructed decision, publishes its record through the
/// PRODUCTION diagnostics writer, and emits the scorer-shaped response
/// line, timing the three phases exactly where the scorer's own serving
/// loop does.
///
/// The phase boundaries, in order, and why each is where it is:
///
/// 1. `decision_micros` opens at the flat encode, which is where a live
///    decision's own work opens, and closes when the finished V4 record is
///    handed to the writer. The fast-forward that repositioned the session
///    is outside it, deliberately: it is an artifact of replaying a corpus
///    and no live decision does it.
/// 2. The writer measures the publication itself, from the moment it takes
///    the record through the durable move. Nothing here times that; the
///    writer is the only thing that can, and it is the same writer a panel
///    runs.
/// 3. The response tail is measured by `note_request_completed_v4` from
///    publication completion, so it covers exactly the serialization, the
///    write and the flush below and nothing else.
///
/// The sum of the three is the latency a client waits for, and it is read
/// back off the published file rather than accumulated here.
#[allow(clippy::too_many_arguments)]
fn search_and_publish_one_decision_v1(
    scorer: &dyn TtsS1DecisionScorerV1,
    bound: &BoundModelGuidedSearchV1,
    value_domain: &crate::model_guided_search_value_quantization_v1::ModelGuidedSearchValueHeadDomainV1,
    session: &FastActorSessionV1,
    expected: FastActorDecisionV1,
    target: &TtsS1CorpusDecisionV1,
    context: &TtsS1DecisionContextV1<'_>,
    diagnostics: &mut ModelGuidedSearchOutcomeWriterV4,
    responses: &mut impl Write,
) -> Result<TtsS1ReplayDecisionRecordV1, TtsS1ReplayErrorV1> {
    use crate::async_flat_scored_rollout_v1::FlatScoredFamilyCore;
    use crate::async_flat_scored_rollout_v2::{FlatScoredFamilyV2, OwnedFlatScoringDecisionV2};
    use crate::flat_policy_v2::FlatDecisionEncoderV2;
    use crate::native_flat_tensorizer_v2::{NativeFlatDecisionTensorV2, NativeFlatTensorizerV2};

    let decision_started = Instant::now();
    let (score, model_input_sha256, candidate_order_commitment) = {
        let mut encoder = FlatDecisionEncoderV2::default();
        let packet = FlatScoredFamilyV2::encode_packet(
            session,
            expected,
            &mut encoder,
            OwnedFlatScoringDecisionV2::default(),
        )
        .map_err(|()| TtsS1ReplayErrorV1::Encode)?;
        let binding = FlatScoredFamilyV2::packet_binding(&packet);
        let view = FlatScoredFamilyV2::packet_view(&packet);
        // The scorer tensorizes here for its model-input commitment and
        // the scoring path tensorizes again inside the inference handle.
        // Both are reproduced, because both are cost a panel pays.
        let mut tensorizer = NativeFlatTensorizerV2::new();
        let mut tensor = NativeFlatDecisionTensorV2::default();
        tensorizer
            .fill(view, &mut tensor)
            .map_err(|_| TtsS1ReplayErrorV1::Encode)?;
        let model_input_sha256 = model_input_sha256_v1(&tensor);
        let score = scorer
            .score_decision_v1(view)
            .map_err(|()| TtsS1ReplayErrorV1::Score)?;
        let commitment = binding.action_binding.candidate_order_commitment;
        drop(FlatScoredFamilyV2::into_owned_packet(packet));
        (score, model_input_sha256, commitment)
    };
    if score.logits.len() != expected.legal_action_count as usize
        || score.logits.iter().any(|value| !value.is_finite())
        || !score.value.is_finite()
    {
        return Err(TtsS1ReplayErrorV1::ScoreContract);
    }
    let action_semantics = session
        .diagnostic_current_action_semantics()
        .ok_or(TtsS1ReplayErrorV1::ScoreContract)?;
    if action_semantics.len() != score.logits.len() {
        return Err(TtsS1ReplayErrorV1::ScoreContract);
    }
    let action_seed = corpus_policy_sample_seed_v1(
        target.coordinates.episode_base_seed,
        target.coordinates.episode_id,
        expected,
    );
    let policy_sample = FastCategoricalScratch::default()
        .sample(&score.logits, action_seed)
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

    let root_statistics_digest_sha256 = lower_hex_sha256_v4(root_statistics_digest_v4(&full));
    let mut v4 = SearchDecisionRecordV4 {
        // Chain and contract fields are writer-assigned.
        contract: String::new(),
        schema_version: 0,
        record_kind: String::new(),
        record_ordinal: 0,
        previous_record_sha256: String::new(),
        decision_ordinal: 0,
        episode_id: expected.episode_id,
        step: expected.step,
        physical_decision_id: expected.physical_decision_id,
        substep_index: expected.substep_index,
        acting_player: expected.acting_player,
        legal_action_count: expected.legal_action_count,
        search_authority_digest_sha256: bound.authority_digest_sha256.clone(),
        requested_transitions: bound.core.authority().transition_budget,
        actual_transitions: full.transitions_used,
        simulations: full.simulations,
        tree_node_count: full.tree_node_count,
        leaf_census: full.leaf_census,
        root_statistics_digest_sha256: root_statistics_digest_sha256.clone(),
        chosen_action_index: full.selected_index,
        visit_margin: visit_margin_v4(&full),
        policy_sample_index: policy_sample,
        search_overrode_policy_sample: full.selected_index != policy_sample,
        stability: None,
        stability_halves_enabled: false,
        protocol_request_kind: ProtocolRequestKindV4::Step,
        search_ceiling_status: CeilingStatusV4::classify_v4(
            search_micros as f64 / MICROS_PER_SECOND_V1 as f64,
        ),
        wall_time: WallTimeV4 {
            full_search_micros: search_micros,
            stability_half_a_micros: 0,
            stability_half_b_micros: 0,
            search_micros,
            decision_micros: 0,
            previous_record_publish_micros: 0,
            previous_record_response_micros: 0,
        },
    };
    let decision_micros = elapsed_micros_v1(decision_started);
    v4.wall_time.decision_micros = decision_micros;

    // PHASE 2: the writer times its own publication, from here.
    diagnostics
        .write_decision_v4(v4)
        .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;

    // PHASE 3: the response tail, exactly as the serving loop pays it.
    let response = TtsS1ScorerShapedResponseV1 {
        protocol: CHECKPOINT_SHADOW_STDIO_PROTOCOL_V1,
        schema_version: CHECKPOINT_SHADOW_STDIO_SCHEMA_VERSION_V1,
        request_id: format!(
            "tts-s1-{}-{}",
            target.coordinates.episode_id, target.coordinates.decision_ordinal
        ),
        checkpoint: context.identity,
        kind: "decision",
        deck_ids: context.deck_ids,
        randomization_identity: SHADOW_RANDOMIZATION_IDENTITY_V1,
        base_seed_u64_hex: format!("{:016x}", context.base_seed),
        pair_index: context.schedule.pair_index,
        pair_environment_seed_u64_hex: format!("{:016x}", context.schedule.environment_seed),
        episode_id: expected.episode_id,
        step: expected.step,
        environment_revision: expected.environment_revision,
        physical_decision_id: expected.physical_decision_id,
        substep_index: expected.substep_index,
        substep_count: expected.substep_count,
        acting_player: expected.acting_player,
        decision_kind: decision_kind_v1(expected.decision_kind),
        legal_action_count: expected.legal_action_count,
        candidate_seat: context.schedule.learner_seat,
        candidate_controls_current_actor: expected.acting_player == context.schedule.learner_seat,
        candidate_action_seed_u64_hex: format!("{action_seed:016x}"),
        selected_action_index: full.selected_index,
        candidate_order_commitment_128_hex: candidate_order_commitment
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        model_input_commitment: CHECKPOINT_SHADOW_MODEL_INPUT_COMMITMENT_V1,
        model_input_sha256,
        diagnostic_state_hash_u64_hex: format!("{:016x}", session.diagnostic_state_hash()),
        core_environment_hash_u64_hex: format!(
            "{:016x}",
            session.privileged_core_environment_hash()
        ),
        logits_f32_bits: score.logits.iter().map(|value| value.to_bits()).collect(),
        value_f32_bits: score.value.to_bits(),
        action_semantics,
    };
    let line = serde_json::to_string(&response).map_err(|error| {
        TtsS1ReplayErrorV1::Diagnostics(format!("response serialization failed: {error}"))
    })?;
    writeln!(responses, "{line}")
        .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
    responses
        .flush()
        .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
    diagnostics.note_request_completed_v4();

    Ok(TtsS1ReplayDecisionRecordV1 {
        // Chain fields are assigned once every record is finished; see the
        // replay loop.
        record_ordinal: 0,
        previous_record_sha256: String::new(),
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
        root_statistics_digest_sha256,
        visit_margin: visit_margin_v4(&full),
        wall_time: TtsS1DecisionWallTimeV1 {
            search_micros,
            decision_micros,
            // The three writer-observed phases are backfilled from the
            // published episode file; see the replay loop.
            publish_micros: 0,
            response_micros: 0,
            protocol_micros: 0,
        },
        ceilings: TtsS1DecisionCeilingsV1 {
            search: classify_micros_v1(search_micros),
            decision: classify_micros_v1(decision_micros),
            protocol: CeilingStatusV4::WithinSlo,
        },
    })
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
        corpus_body_v1, harvest_episode_v1, TtsS1CorpusSelectionV1, TtsS1EpisodeDecisionStatsV1,
        TtsS1FlatScoreV1,
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
        fn score_decision_v1(
            &self,
            decision: FlatScoringDecisionViewV2<'_>,
        ) -> Result<TtsS1FlatScoreV1, ()> {
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
            Ok(TtsS1FlatScoreV1 {
                logits: output.logits,
                value: output.value,
            })
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

    /// A fresh scratch directory for one replay's production diagnostics
    /// writer. Named by process and a counter so parallel tests never
    /// share one; the writer refuses to open a second episode over an
    /// unclosed one, and sharing a directory would make that a flake.
    fn scratch_diagnostics_dir_v1(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tts-s1-replay-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch diagnostics directory");
        path
    }

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
        let all_decisions = harvest.into_decisions_with_action_sequences_v1();
        let episode_decisions = all_decisions.len() as u64;
        let decisions: Vec<_> = all_decisions
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
                // The whole episode's decision count, which is what the
                // compute-cap projection multiplies by. Taken from the
                // episode this fixture actually played, not invented.
                episode_decisions: TtsS1EpisodeDecisionStatsV1::summarize_v1(&[episode_decisions])
                    .expect("one episode summarizes"),
            },
        ))
        .expect("the fixture corpus seals")
    }

    fn replay_fixture_v1(
        scorer: &RunnerFixedScorerV1,
        corpus: &TtsS1CorpusManifestV1,
        tag: &str,
    ) -> TtsS1ReplayReportV1 {
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let identity = fixture_identity_v1();
        let directory = scratch_diagnostics_dir_v1(tag);
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
            &directory,
        )
        .expect("the fixture replay runs");
        // The production writer really did publish, and the response sink
        // really did receive a line per decision: a protocol latency read
        // off nothing would be a latency of nothing.
        assert_eq!(
            std::fs::read_to_string(directory.join(TTS_S1_RESPONSE_LINES_FILE_V1))
                .expect("the response sink exists")
                .lines()
                .count(),
            body.decisions.len(),
            "one scorer-shaped response line per decision"
        );
        for file in &body.diagnostics_episode_files {
            let bytes = std::fs::read(directory.join(&file.file_name))
                .expect("the published episode file exists");
            assert_eq!(bytes.len() as u64, file.bytes);
            assert!(crate::model_guided_search_outcome_v4::verify_episode_chain_v4(&bytes).is_ok());
        }
        let _ = std::fs::remove_dir_all(&directory);
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
        let report = replay_fixture_v1(&scorer, &corpus, "e2e");
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

        // The PROTOCOL latency is strictly more than the decision phase
        // alone, on every decision: the publication and the response tail
        // are real, measured, non-zero work, which is the whole point of
        // routing S1 through the production writer.
        assert!(body.protocol_wall_time.total_micros >= body.decision_wall_time.total_micros);
        for record in &body.decisions {
            assert_eq!(
                record.wall_time.protocol_micros,
                record.wall_time.decision_micros
                    + record.wall_time.publish_micros
                    + record.wall_time.response_micros
            );
            assert!(
                record.wall_time.publish_micros > 0,
                "the writer must have measured a real publication"
            );
        }
        // One published, chain-valid V4 episode file, committed to by the
        // report.
        assert_eq!(body.diagnostics_episode_files.len(), 1);
        assert_eq!(body.diagnostics_episode_files[0].decision_record_count, 3);
        assert_eq!(body.diagnostics_episode_files[0].sha256.len(), 64);

        // The compute cap carries every input, and the projection is the
        // stated arithmetic rather than a number.
        let cap = &body.compute_cap;
        assert_eq!(cap.s2_root_clusters, 3_072);
        assert_eq!(cap.s2_paired_units_per_root_cluster, 2);
        assert_eq!(cap.s2_wrapped_games, 6_144);
        assert_eq!(cap.s2_workers, 16);
        assert_eq!(cap.cap_worker_hours_milli, 48_000);
        assert!(cap.raw_policy_games_excluded);
        assert_eq!(
            cap.mean_decisions_per_game_milli,
            corpus.body.episode_decisions.mean_decisions_milli
        );
        assert_eq!(
            cap.projected_worker_hours_milli,
            project_s2_worker_hours_milli_v1(
                cap.mean_decisions_per_game_milli,
                cap.mean_protocol_micros
            )
        );
        assert_eq!(
            cap.within_cap,
            cap.projected_worker_hours_milli <= cap.cap_worker_hours_milli
        );

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
        let first = replay_fixture_v1(&scorer, &corpus, "timing-a")
            .canonical_bytes_v1()
            .unwrap();
        let second = replay_fixture_v1(&scorer, &corpus, "timing-b")
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
        let directory = scratch_diagnostics_dir_v1("mismatch");

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
            (
                "decision_kind",
                Box::new(|decision: &mut TtsS1CorpusDecisionV1| {
                    // Any OTHER kind in the closed vocabulary: a surface
                    // decision and an attacker-inclusion decision at the
                    // same width are different questions.
                    decision.surface.decision_kind = if decision.surface.decision_kind == "surface"
                    {
                        "attacker_inclusion".to_owned()
                    } else {
                        "surface".to_owned()
                    };
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
                &directory,
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
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The replay refuses a corpus drawn from a different checkpoint.
    #[test]
    fn a_foreign_corpus_checkpoint_fails_closed_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 1, 3);
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let mut identity = fixture_identity_v1();
        identity.loaded_payload_sha256 = "ff".repeat(32);
        let directory = scratch_diagnostics_dir_v1("foreign");
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
                &directory,
            ),
            Err(TtsS1ReplayErrorV1::CheckpointMismatch)
        ));
        let _ = std::fs::remove_dir_all(&directory);
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
            true,
        );
        assert_eq!(verdict, TtsS1TierVerdictV1::Infeasible);
        assert!(reason.contains("exceeds the 4 s SLO"));

        let (verdict, _) = verdict_v1(
            CeilingStatusV4::WithinSlo,
            CeilingStatusV4::HardTimeoutExceeded,
            true,
            true,
        );
        assert_eq!(
            verdict,
            TtsS1TierVerdictV1::Infeasible,
            "a decision that reached the hard timeout is a product failure"
        );

        let (verdict, _) = verdict_v1(
            CeilingStatusV4::WithinSlo,
            CeilingStatusV4::WithinSlo,
            true,
            true,
        );
        assert_eq!(verdict, TtsS1TierVerdictV1::Feasible);

        let (verdict, reason) = verdict_v1(
            CeilingStatusV4::WithinSlo,
            CeilingStatusV4::WithinSlo,
            true,
            false,
        );
        assert_eq!(
            verdict,
            TtsS1TierVerdictV1::Infeasible,
            "a partial replay never carries a feasibility verdict"
        );
        assert!(reason.contains("partial"));

        // The compute cap is GATING on its own: a tier can sit
        // comfortably inside the latency SLO and still be dropped.
        let (verdict, reason) = verdict_v1(
            CeilingStatusV4::WithinSlo,
            CeilingStatusV4::WithinSlo,
            false,
            true,
        );
        assert_eq!(verdict, TtsS1TierVerdictV1::Infeasible);
        assert!(reason.contains("48 worker-hour compute cap"));

        // Every failing clause is named, not just the first.
        let (_, reason) = verdict_v1(
            CeilingStatusV4::SloExceeded,
            CeilingStatusV4::HardTimeoutExceeded,
            false,
            false,
        );
        for fragment in [
            "partial replay",
            "exceeds the 4 s SLO",
            "hard protocol timeout",
            "48 worker-hour compute cap",
        ] {
            assert!(reason.contains(fragment), "{reason} must name {fragment}");
        }
    }

    #[test]
    fn the_compute_cap_projection_is_the_stated_arithmetic_v1() {
        // 6,144 wrapped games x 300 decisions x 1 s / 16 workers
        //   = 115,200 worker-seconds = 32 worker-hours.
        assert_eq!(project_s2_worker_hours_milli_v1(300_000, 1_000_000), 32_000);
        // Doubling the latency doubles the cost and crosses the cap.
        let doubled = project_s2_worker_hours_milli_v1(300_000, 2_000_000);
        assert_eq!(doubled, 64_000);
        assert!(
            doubled > TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1,
            "the doubled projection must land outside the 48 worker-hour cap"
        );

        let stats = TtsS1EpisodeDecisionStatsV1::summarize_v1(&[300]).unwrap();
        let inside = compute_cap_projection_v1(&stats, 1_000_000, 900_000, 800_000);
        assert!(inside.within_cap);
        assert_eq!(inside.projected_worker_hours_milli, 32_000);
        assert_eq!(inside.s2_wrapped_games, 6_144);
        assert!(inside.raw_policy_games_excluded);
        assert_eq!(inside.mean_decision_micros, 900_000);
        assert_eq!(inside.mean_search_micros, 800_000);

        let outside = compute_cap_projection_v1(&stats, 2_000_000, 1_900_000, 1_800_000);
        assert!(!outside.within_cap);
        assert_eq!(outside.projected_worker_hours_milli, 64_000);
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
                publish_micros: 300 + ordinal,
                response_micros: 100 + ordinal,
                protocol_micros: 2_400 + 3 * ordinal,
            },
            ceilings: TtsS1DecisionCeilingsV1 {
                search: CeilingStatusV4::WithinSlo,
                decision: CeilingStatusV4::WithinSlo,
                protocol: CeilingStatusV4::WithinSlo,
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
        let protocol: Vec<u64> = records
            .iter()
            .map(|record| record.wall_time.protocol_micros)
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
            protocol_wall_time: summarize_micros_v1(&protocol).unwrap(),
            decisions_per_second_milli: 1,
            slo_micros: slo_micros_v1(),
            hard_timeout_micros: hard_timeout_micros_v1(),
            p99_protocol_ceiling_status: CeilingStatusV4::WithinSlo,
            max_protocol_ceiling_status: CeilingStatusV4::WithinSlo,
            diagnostics_episode_files: vec![TtsS1DiagnosticsEpisodeFileV1 {
                episode_id: 0,
                file_name: "episode-synthetic.jsonl".to_owned(),
                bytes: 4_096,
                sha256: "cd".repeat(32),
                decision_record_count: record_count,
            }],
            compute_cap: compute_cap_projection_v1(
                &TtsS1EpisodeDecisionStatsV1::summarize_v1(&[300]).unwrap(),
                2_400,
                2_000,
                1_000,
            ),
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
            record.wall_time.publish_micros = 700_000 + index as u64;
            record.wall_time.response_micros = 300_000 + index as u64;
            record.wall_time.protocol_micros = record.wall_time.decision_micros
                + record.wall_time.publish_micros
                + record.wall_time.response_micros;
            record.ceilings.search = classify_micros_v1(record.wall_time.search_micros);
            record.ceilings.decision = classify_micros_v1(record.wall_time.decision_micros);
            record.ceilings.protocol = classify_micros_v1(record.wall_time.protocol_micros);
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
        let protocol: Vec<u64> = slow
            .decisions
            .iter()
            .map(|record| record.wall_time.protocol_micros)
            .collect();
        slow.search_wall_time = summarize_micros_v1(&search).unwrap();
        slow.decision_wall_time = summarize_micros_v1(&decision).unwrap();
        slow.protocol_wall_time = summarize_micros_v1(&protocol).unwrap();
        slow.p99_protocol_ceiling_status = classify_micros_v1(slow.protocol_wall_time.p99_micros);
        slow.max_protocol_ceiling_status = classify_micros_v1(slow.protocol_wall_time.max_micros);
        slow.compute_cap = compute_cap_projection_v1(
            &TtsS1EpisodeDecisionStatsV1::summarize_v1(&[300]).unwrap(),
            slow.protocol_wall_time.total_micros / slow.decisions.len() as u64,
            slow.decision_wall_time.total_micros / slow.decisions.len() as u64,
            slow.search_wall_time.total_micros / slow.decisions.len() as u64,
        );
        let (verdict, reason) = verdict_v1(
            slow.p99_protocol_ceiling_status,
            slow.max_protocol_ceiling_status,
            slow.compute_cap.within_cap,
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
