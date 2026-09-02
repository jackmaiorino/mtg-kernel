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
    publish_canonical_document_v1, TtsS1AllEpisodeDecisionStatsV1, TtsS1CorpusCheckpointV1,
    TtsS1CorpusDecisionV1, TtsS1CorpusEpisodeV1, TtsS1CorpusErrorV1, TtsS1CorpusManifestV1,
    TtsS1DecisionScorerV1, TTS_S1_NEAREST_RANK_PERCENTILE_RULE_V1,
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

/// Wire schema of ONE SHARD's partial report.
///
/// Deliberately a different string from
/// [`TTS_S1_REPLAY_REPORT_SCHEMA_V1`], not a flag inside the same one: a
/// shard report carries no verdict, no view and no compute-cap projection,
/// because a fraction of the episodes cannot produce any of the three. A
/// reader that mistook one for the other would be reading a partial
/// measurement as a tier's feasibility, so the two are made
/// unrepresentable as each other rather than merely distinguishable.
pub const TTS_S1_REPLAY_SHARD_REPORT_SCHEMA_V1: &str = "mtg-kernel-tts-s1-replay-shard-report/v1";

/// The largest shard count a tier may be split into.
///
/// A bound, not a tuning knob. Sharding is a pure execution split (see
/// [`TtsS1ShardSelectorV1`]), so a larger count changes nothing about WHICH
/// decisions are measured; but every shard is a whole process holding a
/// loaded checkpoint, and a mis-typed count must fail closed at
/// configuration time rather than fork a thousand of them.
pub const TTS_S1_MAX_SHARD_COUNT_V1: u64 = 64;

/// THE PINNED FORMAL CONCURRENCY.
///
/// Sharding does not change which decisions are measured, but it does
/// change the machine those measurements were taken on: eight replay
/// processes contend for cores, memory bandwidth and the disk the
/// production diagnostics writer republishes an episode file to after every
/// decision. The p99 protocol latency, the hard-timeout clause and the
/// isotonic curve the compute-cap projection is fitted to are all wall-time
/// samples, so a run at a different concurrency is a measurement of a
/// different machine, and `-ShardCount` would otherwise be a knob that can
/// flip a tier's verdict.
///
/// So the formal topology is pinned rather than chosen: eight, because the
/// CP7 panel host runs the wrapped agent under eight concurrent games, and
/// a formal S1 latency claim has to be measured at the concurrency the
/// product is actually served at. A run at any other count is a SMOKE: it
/// still replays, still publishes every report and still carries a verdict
/// in its own report, and it may never be read as a feasibility result.
pub const TTS_S1_FORMAL_SHARD_COUNT_V1: u64 = 8;

/// Logical CPUs a formal run requires PER SHARD.
///
/// Two, so eight concurrent replay processes have sixteen logical CPUs
/// between them. Below that the shards are time-slicing rather than
/// running, and every latency measured is a measurement of the contention
/// and not of the tier.
pub const TTS_S1_FORMAL_LOGICAL_CPUS_PER_SHARD_V1: u64 = 2;

/// The topology rule, spelled out on the wire so a reader never has to
/// infer which concurrency a tier's timings were taken under, or what would
/// have made them formal.
pub const TTS_S1_SHARD_TOPOLOGY_RULE_V1: &str = concat!(
    "formal-s1-timings-are-measured-at-exactly-8-concurrent-replay-processes",
    "-the-concurrency-the-cp7-panel-host-runs-the-wrapped-agent-under",
    "-on-a-host-with-at-least-2-logical-cpus-per-shard",
    "-any-other-shard-count-or-a-smaller-host-is-a-smoke-and-never-a-feasibility-result",
    "/v1"
);

/// How a shard's episode subset is defined, stated on the wire so nobody
/// has to reconstruct it from the episode list.
pub const TTS_S1_SHARD_ASSIGNMENT_RULE_V1: &str =
    "contributing-episode-position-in-corpus-order-modulo-shard-count-equals-shard-index/v1";

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
///
/// V2, and the version is not cosmetic: V1's text declared a
/// mean-length-times-mean-latency rule, which is not what is computed any
/// more and was wrong for two reasons (it mixed populations, and a flat
/// mean cannot cost an episode longer than the ones measured). A rule
/// string that describes a computation the code no longer performs is worse
/// than no rule string, because it is believed.
///
/// Note what is NOT in it: a division by the worker count. WORKER-hours are
/// aggregate work, so 6,144 games costing 300 s each are 512 worker-hours
/// however many workers run them. Dividing by 16 would give ELAPSED hours
/// on a full host, which is a different quantity against a different
/// threshold; it is published separately, as
/// `projected_elapsed_hours_at_workers_milli`, and the cap is checked
/// against the worker-hours.
pub const TTS_S1_S2_PROJECTION_RULE_V2: &str = concat!(
    "wrapped-games-only",
    "-3072-root-clusters-times-2-paired-units",
    "-isotonic-per-ordinal-protocol-latency-curve-fitted-to-whole-episode-timings",
    "-extrapolated-past-the-last-observed-ordinal-at-the-maximum-adjacent-fitted-rise",
    "-every-harvested-episode-natural-and-truncated-costed-at-its-own-length",
    "-mean-estimated-episode-cost-times-wrapped-games",
    "-as-aggregate-worker-hours-with-no-worker-division",
    "/v2"
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

/// Which slice of the contributing episodes one replay process owns.
///
/// # Sharding is an EXECUTION split and nothing else
///
/// The measured population, the two views, the isotonic curve, the
/// per-episode cost estimates, the compute-cap projection and the verdict
/// are all defined over the SAME set of decisions whether one process or
/// sixty-four produced them, because every one of them is a function of
/// the record set and not of the order or the process the records arrived
/// in. What sharding changes is which process pays for which episode; see
/// [`merge_tts_s1_replay_shards_v1`], which recomputes every statistic over
/// the union through the same functions the unsharded path calls, so the
/// merged report is the unsharded report.
///
/// The split is by EPISODE, never by decision, and that is forced rather
/// than chosen: the production diagnostics writer republishes the whole
/// episode file after every decision, so a decision's publication cost is a
/// function of every earlier searched decision IN ITS EPISODE. Two
/// processes splitting one episode would each publish a short file and
/// measure a publication phase no panel ever pays.
///
/// A shard owns the contributing episodes whose POSITION in the corpus's
/// own deterministic episode order (ascending episode id, which
/// `decode_tts_s1_corpus_v1` proved) satisfies `position % count == index`.
/// Round-robin rather than contiguous blocks, deliberately: episode length
/// varies by a factor of several, and contiguous blocks would hand one
/// process a run of long games while another finished early.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ShardSelectorV1 {
    pub shard_index: u64,
    pub shard_count: u64,
}

impl TtsS1ShardSelectorV1 {
    /// The only constructor, so an out-of-range shard is unrepresentable
    /// rather than merely rejected somewhere later.
    pub fn new_v1(shard_index: u64, shard_count: u64) -> Option<Self> {
        if shard_count == 0 || shard_count > TTS_S1_MAX_SHARD_COUNT_V1 || shard_index >= shard_count
        {
            return None;
        }
        Some(Self {
            shard_index,
            shard_count,
        })
    }

    /// Whether this shard replays the episode at `position` in the corpus's
    /// episode order.
    pub const fn owns_position_v1(&self, position: u64) -> bool {
        position % self.shard_count == self.shard_index
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
    /// Whether this decision is one of the stratified corpus targets.
    /// EVERY decision of a contributing episode is searched and recorded;
    /// this is what separates the two published views.
    pub is_corpus_target: bool,
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

/// One block of the fitted per-ordinal latency curve: a maximal run of
/// decision ordinals the isotonic fit assigned a single value.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1LatencyCurveKnotV1 {
    pub first_ordinal: u64,
    pub last_ordinal: u64,
    /// The block's pooled mean protocol latency, floored to micros.
    pub fitted_micros: u64,
    /// How many replayed decisions fell in this block.
    pub sample_count: u64,
}

/// A monotone non-decreasing per-ordinal protocol-latency curve, fitted to
/// the whole-episode replay and extrapolated past the last ordinal it
/// observed.
///
/// # Why a curve and not a mean
///
/// The production diagnostics writer republishes the whole episode file
/// after every decision, so publication cost GROWS with the decision
/// ordinal. A single mean latency therefore cannot cost an episode that is
/// longer than the ones measured: it would charge the long tail at the
/// average of a short one. It also cannot be multiplied by a decision count
/// drawn from a different population than the timings, which is what
/// pairing an all-episodes mean length with contributing-episode timings
/// did.
///
/// # The fit
///
/// Every replayed decision arrives as `(decision ordinal, protocol
/// micros)`, pooled across every replayed episode, so the SAME ordinal
/// carries one sample per episode. Those are PRE-AGGREGATED into one
/// `(sum, count)` point per ordinal BEFORE pool-adjacent-violators runs.
/// That order matters: folding a repeated ordinal into the running block
/// without re-running the violation check lets a low later sample at an
/// already-seen ordinal drag that block below its predecessor and leave the
/// output non-monotone, which is not an isotonic fit at all. With the
/// aggregation first, PAV sees a strictly increasing sequence of ordinals
/// and its output is monotone by construction.
///
/// The result is the least-squares monotone non-decreasing fit, expressed
/// as maximal constant blocks. The comparison that decides a merge
/// cross-multiplies the blocks' `(sum, count)` pairs, so no rounding enters
/// the shape of the fit; only the published `fitted_micros` is floored.
///
/// # The extrapolation
///
/// Beyond `last_observed_ordinal` the curve continues at
/// `extrapolation_slope_micros_per_ordinal`, the LARGEST rise between two
/// ADJACENT fitted ordinals. In a step function that rise happens entirely
/// at a block boundary, over a distance of one ordinal, so it is the jump
/// between consecutive blocks' fitted values and nothing is divided by the
/// blocks' width: dividing by the distance between block ends spreads a
/// one-ordinal step across the whole preceding block and reports a smaller
/// slope than the curve actually takes.
///
/// It is deliberately the steepest evidence available rather than an
/// average one, because the estimate is meant to be conservative about
/// episodes longer than anything replayed. It is floored at 1 micro per
/// ordinal so the curve is never flat past its evidence: a flat tail would
/// cost an arbitrarily long episode at the last measured rate, which is the
/// exact understatement the growth-with-history property makes wrong.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1LatencyCurveV1 {
    pub rule: String,
    pub knots: Vec<TtsS1LatencyCurveKnotV1>,
    pub last_observed_ordinal: u64,
    pub extrapolation_slope_micros_per_ordinal: u64,
    pub observed_samples: u64,
}

/// Which published view the SLO and hard-timeout verdict is taken from.
///
/// The sketch defines S1's measured population as the FROZEN STRATIFIED
/// CORPUS (Section 5, S1: "a FROZEN stratified corpus of >= 512 decisions
/// ... replayed per tier; records p50/p99/max wall time"), so the gate is
/// the corpus targets. The whole-episode replay exists to give those
/// targets a realistic publication history, not to replace them as the
/// population; its own percentiles are published as a diagnostic.
pub const TTS_S1_VERDICT_VIEW_V1: &str = "corpus_target_view";

/// The fitted curve's own identity, on the wire.
///
/// V2: V1's text said "maximum fitted slope", which was ambiguous and which
/// the code read as a rise divided by the distance between block ENDS. That
/// understates the largest step the fitted curve actually takes, because
/// the step happens in a single ordinal at a block boundary and the
/// division spread it over the whole preceding block. The rule now says
/// what is measured: the largest rise between two ADJACENT fitted ordinals.
pub const TTS_S1_LATENCY_CURVE_RULE_V2: &str = concat!(
    "pool-adjacent-violators-isotonic-regression-over-decision-ordinal",
    "-on-whole-episode-protocol-micros-pre-aggregated-per-ordinal",
    "-extrapolated-past-the-last-observed-ordinal",
    "-at-the-maximum-rise-between-adjacent-fitted-ordinals",
    "-floored-at-one-micro-per-ordinal",
    "/v2"
);

impl TtsS1LatencyCurveV1 {
    /// Fits the curve to `(decision ordinal, protocol micros)` samples.
    pub fn fit_v1(samples: &[(u64, u64)]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        // STEP 1: aggregate by ordinal, ascending, BEFORE any pooling.
        // Several episodes contribute a sample at the same ordinal; they
        // become one (sum, count) point. Doing this first is what makes
        // the fit monotone: folding a repeated ordinal into a running
        // block without re-checking for a violation can leave that block
        // below its predecessor.
        let mut ordered = samples.to_vec();
        ordered.sort_unstable_by_key(|(ordinal, _)| *ordinal);
        // (ordinal, sum, count), strictly increasing in ordinal.
        let mut points: Vec<(u64, u128, u64)> = Vec::new();
        for (ordinal, micros) in ordered {
            match points.last_mut() {
                Some(last) if last.0 == ordinal => {
                    last.1 = last.1.saturating_add(u128::from(micros));
                    last.2 = last.2.saturating_add(1);
                }
                _ => points.push((ordinal, u128::from(micros), 1)),
            }
        }
        // STEP 2: pool adjacent violators over those points. Merge back
        // while the previous block's mean exceeds this one's, compared by
        // cross-multiplication so the fit's shape is exact.
        // (first_ordinal, last_ordinal, sum, count)
        let mut blocks: Vec<(u64, u64, u128, u64)> = Vec::new();
        for (ordinal, sum, count) in points {
            blocks.push((ordinal, ordinal, sum, count));
            while blocks.len() >= 2 {
                let (_, _, sum_b, count_b) = blocks[blocks.len() - 1];
                let (_, _, sum_a, count_a) = blocks[blocks.len() - 2];
                if sum_a.saturating_mul(u128::from(count_b))
                    <= sum_b.saturating_mul(u128::from(count_a))
                {
                    break;
                }
                let (_, last_b, sum_b, count_b) = blocks.pop()?;
                let merged = blocks.last_mut()?;
                merged.1 = last_b.max(merged.1);
                merged.2 = merged.2.saturating_add(sum_b);
                merged.3 = merged.3.saturating_add(count_b);
            }
        }
        let knots: Vec<TtsS1LatencyCurveKnotV1> = blocks
            .iter()
            .map(|(first, last, sum, count)| TtsS1LatencyCurveKnotV1 {
                first_ordinal: *first,
                last_ordinal: *last,
                fitted_micros: u64::try_from(sum / u128::from(*count)).unwrap_or(u64::MAX),
                sample_count: *count,
            })
            .collect();
        let last_observed_ordinal = knots.last()?.last_ordinal;
        // The largest rise between two ADJACENT fitted ordinals. In a step
        // function that rise is the jump at a block boundary, which spans
        // exactly one ordinal, so nothing is divided by the blocks' width:
        // dividing by the distance between block ends would spread a
        // one-ordinal step over the whole preceding block and report a
        // slope smaller than the curve actually takes. Floored at one micro
        // so the tail is never flat.
        let mut slope = 1u64;
        for pair in knots.windows(2) {
            slope = slope.max(pair[1].fitted_micros.saturating_sub(pair[0].fitted_micros));
        }
        Some(Self {
            rule: TTS_S1_LATENCY_CURVE_RULE_V2.to_owned(),
            observed_samples: samples.len() as u64,
            knots,
            last_observed_ordinal,
            extrapolation_slope_micros_per_ordinal: slope,
        })
    }

    /// The fitted latency at one decision ordinal, extrapolating past the
    /// last observed one.
    pub fn latency_at_v1(&self, ordinal: u64) -> u64 {
        let Some(last) = self.knots.last() else {
            return 0;
        };
        if ordinal > self.last_observed_ordinal {
            return last.fitted_micros.saturating_add(
                self.extrapolation_slope_micros_per_ordinal
                    .saturating_mul(ordinal - self.last_observed_ordinal),
            );
        }
        // Below the first observed ordinal the curve holds its first
        // value; a corpus episode always starts at ordinal 0, so this only
        // arises if the replay never saw ordinal 0.
        for knot in &self.knots {
            if ordinal <= knot.last_ordinal {
                return knot.fitted_micros;
            }
        }
        last.fitted_micros
    }

    /// The estimated cost of one whole episode of `decision_count`
    /// decisions: the fitted latency summed over ordinals `0..decision_count`.
    ///
    /// Returns the cost in micros and how many of those ordinals fell past
    /// the last observed one, so the estimate says how much of itself is
    /// extrapolation.
    pub fn episode_cost_micros_v1(&self, decision_count: u64) -> (u128, u64) {
        let mut cost = 0u128;
        let mut extrapolated = 0u64;
        for ordinal in 0..decision_count {
            cost = cost.saturating_add(u128::from(self.latency_at_v1(ordinal)));
            if ordinal > self.last_observed_ordinal {
                extrapolated += 1;
            }
        }
        (cost, extrapolated)
    }
}

/// One harvested episode's estimated whole-episode cost.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1EpisodeCostEstimateV1 {
    /// Index into the corpus's `all_episode_decisions.decision_counts`,
    /// which is in ascending episode id.
    pub episode_index: u64,
    pub decision_count: u64,
    pub estimated_micros: u64,
    /// How many of this episode's ordinals were past anything replayed.
    pub extrapolated_ordinals: u64,
}

/// Sketch Section 4's compute cap, with every input to the estimate.
///
/// The rule, fixed here and restated on the wire:
///
/// ```text
/// curve(o)        = isotonic fit of protocol micros over decision ordinal,
///                   extrapolated past the last observed ordinal at the
///                   maximum fitted slope
/// cost(episode)   = sum of curve(o) for o in 0..episode decisions
/// worker_hours    = mean cost(episode) over EVERY harvested episode
///                 x 6,144 wrapped games
/// ```
///
/// No division by the worker count: worker-hours are aggregate work, and
/// 6,144 games of 300 decisions at 1 s each are 512 worker-hours however
/// many workers run them. The 16-worker elapsed figure is published beside
/// it as information.
///
/// The two populations now agree. Every harvested episode is costed, natural
/// or truncated, contributing or not, and each is costed at ITS OWN length
/// against a curve fitted to the replay's own per-ordinal timings. The
/// earlier form multiplied an all-episodes mean LENGTH by a
/// contributing-episodes mean LATENCY, which are different populations, and
/// a flat mean latency cannot cost an episode longer than the ones measured
/// at all, because publication cost grows with the ordinal.
///
/// One conservative reading remains and is recorded: every decision count
/// here is decisions by BOTH seats, because that is what a self-play episode
/// contains, while an S2 wrapped agent occupies one seat. The estimate is an
/// over-estimate on that axis.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ComputeCapProjectionV1 {
    pub rule: String,
    pub s2_root_clusters: u64,
    pub s2_paired_units_per_root_cluster: u64,
    pub s2_wrapped_games: u64,
    pub s2_workers: u64,
    /// The fitted curve, with its knots and its extrapolation slope.
    pub latency_curve: TtsS1LatencyCurveV1,
    /// Every harvested episode's estimated cost, individually.
    pub episode_cost_estimates: Vec<TtsS1EpisodeCostEstimateV1>,
    /// The population that was costed: natural plus truncated.
    pub estimated_episode_count: u64,
    pub natural_terminal_episode_count: u64,
    pub truncated_episode_count: u64,
    /// How many ordinals across the whole population fell past anything
    /// replayed, so a reader can see how much of the estimate is
    /// extrapolation.
    pub extrapolated_ordinals: u64,
    pub mean_estimated_episode_micros: u64,
    pub max_estimated_episode_micros: u64,
    /// Context: the observed lengths the estimate is taken over.
    pub p99_decisions_per_game: u64,
    pub max_decisions_per_game: u64,
    pub raw_policy_games_excluded: bool,
    pub raw_policy_exclusion_reason: String,
    /// Aggregate work. THIS is what the cap is checked against.
    pub projected_worker_hours_milli: u64,
    /// The same cost as elapsed hours on the 16-worker host.
    /// Informational.
    pub projected_elapsed_hours_at_workers_milli: u64,
    pub cap_worker_hours_milli: u64,
    pub within_cap: bool,
}

/// Aggregate worker-hours (times 1,000) from a mean per-episode cost.
///
/// There is deliberately no division by the worker count here: the product
/// of games and per-game cost IS the aggregate work. See
/// [`project_s2_elapsed_hours_milli_v1`] for the elapsed figure.
pub fn project_s2_worker_hours_milli_v1(mean_episode_micros: u64) -> u64 {
    let wrapped_games = u128::from(TTS_S1_S2_ROOT_CLUSTERS_V1)
        .saturating_mul(u128::from(TTS_S1_S2_PAIRED_UNITS_PER_ROOT_CLUSTER_V1));
    let total_micros = wrapped_games.saturating_mul(u128::from(mean_episode_micros));
    // micros -> hours, times 1,000: divide by 3.6e9, multiply by 1e3.
    let milli = total_micros.saturating_mul(1_000) / 3_600_000_000u128;
    u64::try_from(milli).unwrap_or(u64::MAX)
}

/// The same cost as ELAPSED hours on the pre-registered 16-worker host.
/// Informational: the cap is a worker-hour cap and is checked against
/// [`project_s2_worker_hours_milli_v1`].
pub fn project_s2_elapsed_hours_milli_v1(worker_hours_milli: u64) -> u64 {
    worker_hours_milli / TTS_S1_S2_WORKERS_V1
}

/// Builds the projection block for one tier.
///
/// `samples` are the whole-episode replay's `(decision ordinal, protocol
/// micros)` pairs; `all_episode_decisions` is the corpus's whole harvested
/// population, every member of which is costed.
pub fn compute_cap_projection_v1(
    samples: &[(u64, u64)],
    all_episode_decisions: &crate::native_tts_s1_corpus_v1::TtsS1AllEpisodeDecisionStatsV1,
) -> Option<TtsS1ComputeCapProjectionV1> {
    let latency_curve = TtsS1LatencyCurveV1::fit_v1(samples)?;
    let mut episode_cost_estimates =
        Vec::with_capacity(all_episode_decisions.decision_counts.len());
    let mut total_micros = 0u128;
    let mut extrapolated_ordinals = 0u64;
    let mut max_estimated_episode_micros = 0u64;
    for (index, decision_count) in all_episode_decisions.decision_counts.iter().enumerate() {
        let (cost, extrapolated) = latency_curve.episode_cost_micros_v1(*decision_count);
        let estimated_micros = u64::try_from(cost).unwrap_or(u64::MAX);
        total_micros = total_micros.saturating_add(cost);
        extrapolated_ordinals = extrapolated_ordinals.saturating_add(extrapolated);
        max_estimated_episode_micros = max_estimated_episode_micros.max(estimated_micros);
        episode_cost_estimates.push(TtsS1EpisodeCostEstimateV1 {
            episode_index: index as u64,
            decision_count: *decision_count,
            estimated_micros,
            extrapolated_ordinals: extrapolated,
        });
    }
    let episode_count = all_episode_decisions.episode_count;
    if episode_count == 0 {
        return None;
    }
    let mean_estimated_episode_micros =
        u64::try_from(total_micros / u128::from(episode_count)).unwrap_or(u64::MAX);
    let projected_worker_hours_milli =
        project_s2_worker_hours_milli_v1(mean_estimated_episode_micros);
    Some(TtsS1ComputeCapProjectionV1 {
        rule: TTS_S1_S2_PROJECTION_RULE_V2.to_owned(),
        s2_root_clusters: TTS_S1_S2_ROOT_CLUSTERS_V1,
        s2_paired_units_per_root_cluster: TTS_S1_S2_PAIRED_UNITS_PER_ROOT_CLUSTER_V1,
        s2_wrapped_games: TTS_S1_S2_ROOT_CLUSTERS_V1
            .saturating_mul(TTS_S1_S2_PAIRED_UNITS_PER_ROOT_CLUSTER_V1),
        s2_workers: TTS_S1_S2_WORKERS_V1,
        latency_curve,
        episode_cost_estimates,
        estimated_episode_count: episode_count,
        natural_terminal_episode_count: all_episode_decisions.natural_terminal_episode_count,
        truncated_episode_count: all_episode_decisions.truncated_episode_count,
        extrapolated_ordinals,
        mean_estimated_episode_micros,
        max_estimated_episode_micros,
        p99_decisions_per_game: all_episode_decisions.p99_decisions,
        max_decisions_per_game: all_episode_decisions.max_decisions,
        raw_policy_games_excluded: true,
        raw_policy_exclusion_reason: TTS_S1_RAW_POLICY_EXCLUSION_REASON_V1.to_owned(),
        projected_worker_hours_milli,
        projected_elapsed_hours_at_workers_milli: project_s2_elapsed_hours_milli_v1(
            projected_worker_hours_milli,
        ),
        cap_worker_hours_milli: TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1,
        within_cap: projected_worker_hours_milli <= TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1,
    })
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

/// One population's latency statistics.
///
/// Two are published and they answer different questions. The
/// WHOLE-EPISODE view covers every decision searched, which is the
/// population a panel actually plays and therefore the one the SLO verdict
/// and the compute projection are taken from. The CORPUS-TARGET view
/// covers the 512 stratified decisions alone: useful for reading the
/// strata (high branching, stack interaction, combat, late game) against
/// each other, and useless as a mean, because a quota-stratified sample is
/// deliberately not representative of a game.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1LatencyViewV1 {
    pub decisions: u64,
    /// DIAGNOSTIC: the search alone.
    pub search_wall_time: TtsS1TimingSummaryV1,
    /// DIAGNOSTIC: the scorer's `decision_micros` phase alone.
    pub decision_wall_time: TtsS1TimingSummaryV1,
    /// Decision plus the production writer's own publication measurement
    /// plus the measured response tail.
    pub protocol_wall_time: TtsS1TimingSummaryV1,
    pub mean_protocol_micros: u64,
    /// Decisions per second times 1,000, floored, over the protocol total.
    pub decisions_per_second_milli: u64,
    pub p99_protocol_ceiling_status: CeilingStatusV4,
    pub max_protocol_ceiling_status: CeilingStatusV4,
}

/// Summarizes one population of records.
pub fn latency_view_v1(records: &[&TtsS1ReplayDecisionRecordV1]) -> Option<TtsS1LatencyViewV1> {
    if records.is_empty() {
        return None;
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
    let search_wall_time = summarize_micros_v1(&search)?;
    let decision_wall_time = summarize_micros_v1(&decision)?;
    let protocol_wall_time = summarize_micros_v1(&protocol)?;
    let decisions = records.len() as u64;
    Some(TtsS1LatencyViewV1 {
        decisions,
        mean_protocol_micros: protocol_wall_time.total_micros / decisions,
        decisions_per_second_milli: if protocol_wall_time.total_micros == 0 {
            0
        } else {
            decisions
                .saturating_mul(MICROS_PER_SECOND_V1)
                .saturating_mul(1_000)
                / protocol_wall_time.total_micros
        },
        p99_protocol_ceiling_status: classify_micros_v1(protocol_wall_time.p99_micros),
        max_protocol_ceiling_status: classify_micros_v1(protocol_wall_time.max_micros),
        search_wall_time,
        decision_wall_time,
        protocol_wall_time,
    })
}

/// What the measuring process observed about the host it ran on.
///
/// Read ONCE, at launch, before anything is measured, and read-only: this
/// module never sizes anything from it and never branches on it. It exists
/// so the topology a tier's timings were taken under is auditable from the
/// report alone, and so the formal-run rule
/// ([`TTS_S1_FORMAL_LOGICAL_CPUS_PER_SHARD_V1`]) is checkable against a
/// recorded fact instead of against a claim about the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TtsS1HostFactsV1 {
    pub logical_cpus: u64,
    /// Total physical memory in bytes, or 0 where the platform did not
    /// answer. Zero is published as zero rather than omitted: a reader has
    /// to be able to tell "the host had none to report" from "nobody
    /// asked", and a missing field cannot.
    pub total_memory_bytes: u64,
}

impl TtsS1HostFactsV1 {
    /// Reads the host's logical CPU count and total physical memory.
    pub fn read_v1() -> Self {
        Self {
            logical_cpus: std::thread::available_parallelism()
                .map(|count| count.get() as u64)
                .unwrap_or(0),
            total_memory_bytes: host_total_memory_v1::total_memory_bytes_v1().unwrap_or(0),
        }
    }
}

#[cfg(windows)]
mod host_total_memory_v1 {
    /// `MEMORYSTATUSEX`, field for field and in order.
    #[repr(C)]
    #[allow(non_snake_case)]
    struct MemoryStatusExV1 {
        dwLength: u32,
        dwMemoryLoad: u32,
        ullTotalPhys: u64,
        ullAvailPhys: u64,
        ullTotalPageFile: u64,
        ullAvailPageFile: u64,
        ullTotalVirtual: u64,
        ullAvailVirtual: u64,
        ullAvailExtendedVirtual: u64,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusExV1) -> i32;
    }

    pub(super) fn total_memory_bytes_v1() -> Option<u64> {
        let mut status = MemoryStatusExV1 {
            dwLength: u32::try_from(std::mem::size_of::<MemoryStatusExV1>()).ok()?,
            dwMemoryLoad: 0,
            ullTotalPhys: 0,
            ullAvailPhys: 0,
            ullTotalPageFile: 0,
            ullAvailPageFile: 0,
            ullTotalVirtual: 0,
            ullAvailVirtual: 0,
            ullAvailExtendedVirtual: 0,
        };
        // SAFETY: `status` is a plain-old-data `#[repr(C)]` struct whose
        // `dwLength` is set to its own size before the call, which is the
        // whole of the Win32 contract for `MEMORYSTATUSEX`. The call reads
        // and writes only that struct.
        let succeeded = unsafe { GlobalMemoryStatusEx(&mut status) != 0 };
        succeeded.then_some(status.ullTotalPhys)
    }
}

#[cfg(all(unix, not(windows)))]
mod host_total_memory_v1 {
    pub(super) fn total_memory_bytes_v1() -> Option<u64> {
        // `MemTotal:` in kibibytes, which is the only line needed and the
        // only one parsed.
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kibibytes: u64 = text
            .lines()
            .find_map(|line| line.strip_prefix("MemTotal:"))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()?;
        kibibytes.checked_mul(1_024)
    }
}

#[cfg(not(any(windows, unix)))]
mod host_total_memory_v1 {
    pub(super) fn total_memory_bytes_v1() -> Option<u64> {
        None
    }
}

/// The concurrency a tier's timings were measured under, and whether that
/// topology may carry formal standing.
///
/// Recorded, never acted on here: nothing in this module reads
/// `meets_formal_topology` to decide anything, exactly as nothing reads a
/// duration. The launcher is what refuses to mark a run complete, and it
/// reads this field rather than its own flags, so a report is the authority
/// on the topology it was produced under.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ShardTopologyV1 {
    pub rule: String,
    /// Replay processes that produced these timings. One for an unsharded
    /// run.
    pub shard_count: u64,
    /// [`TTS_S1_FORMAL_SHARD_COUNT_V1`], on the wire so a reader sees the
    /// pin beside the count rather than having to know it.
    pub formal_shard_count: u64,
    pub formal_logical_cpus_per_shard: u64,
    /// Logical CPUs the measuring processes observed.
    pub host_logical_cpus: u64,
    /// Total physical memory the measuring processes observed, in bytes.
    /// Zero means the platform did not answer.
    pub host_total_memory_bytes: u64,
    /// Whether this run's topology is the pinned formal one on a host large
    /// enough for it.
    pub meets_formal_topology: bool,
    /// Every failing clause, or the sentence that says none failed.
    pub formal_topology_reason: String,
}

impl TtsS1ShardTopologyV1 {
    /// Builds the block from the observed host and the concurrency used.
    pub fn evaluate_v1(shard_count: u64, host: TtsS1HostFactsV1) -> Self {
        let required_cpus = TTS_S1_FORMAL_LOGICAL_CPUS_PER_SHARD_V1.saturating_mul(shard_count);
        let mut failures: Vec<String> = Vec::new();
        if shard_count != TTS_S1_FORMAL_SHARD_COUNT_V1 {
            failures.push(format!(
                "the timings were measured at {shard_count} concurrent replay processes, not the pinned {TTS_S1_FORMAL_SHARD_COUNT_V1}"
            ));
        }
        if host.logical_cpus < required_cpus {
            failures.push(format!(
                "the host reported {} logical CPUs, below the {required_cpus} a {shard_count}-shard run requires at {TTS_S1_FORMAL_LOGICAL_CPUS_PER_SHARD_V1} per shard",
                host.logical_cpus
            ));
        }
        let meets_formal_topology = failures.is_empty();
        let formal_topology_reason = if meets_formal_topology {
            format!(
                "the timings were measured at the pinned {TTS_S1_FORMAL_SHARD_COUNT_V1} concurrent replay processes on a host reporting {} logical CPUs, at or above the {required_cpus} required",
                host.logical_cpus
            )
        } else {
            failures.join("; ")
        };
        Self {
            rule: TTS_S1_SHARD_TOPOLOGY_RULE_V1.to_owned(),
            shard_count,
            formal_shard_count: TTS_S1_FORMAL_SHARD_COUNT_V1,
            formal_logical_cpus_per_shard: TTS_S1_FORMAL_LOGICAL_CPUS_PER_SHARD_V1,
            host_logical_cpus: host.logical_cpus,
            host_total_memory_bytes: host.total_memory_bytes,
            meets_formal_topology,
            formal_topology_reason,
        }
    }
}

/// Everything about a tier replay that is the SAME in every shard of it.
///
/// It exists so the two paths cannot drift. The unsharded replay builds one
/// and finalizes its report from it; every shard publishes its own copy;
/// and the merge refuses to proceed unless all K are equal to each other,
/// equal to the merging binary's own compiled constants, and equal to its
/// own `engine_commit`. So "the shards measured the same thing" is a
/// checked fact rather than an assumption about how the launcher was
/// invoked.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ReplayIdentityV1 {
    pub engine_commit: String,
    pub tier: String,
    pub transition_budget: u32,
    pub policy_step_depth_cap: u16,
    pub seed_block_id: u64,
    pub seed_block_seed: u64,
    pub stability_halves_enabled: bool,
    pub checkpoint: TtsS1CorpusCheckpointV1,
    pub wrapper_identity: WrapperIdentityV4,
    pub search_authority_digest_sha256: String,
    pub corpus_sha256: String,
    pub corpus_decision_count: u64,
    pub corpus_episode_count: u64,
    /// Episodes the WHOLE run replays: every shard together, not this one.
    /// It is the unsharded report's `episodes_replayed`, and it is what the
    /// merge checks the union of the shards' positions against.
    pub episodes_replayed: u64,
    pub max_episodes: u64,
    pub percentile_rule: String,
    pub verdict_view: String,
    pub slo_micros: u64,
    pub hard_timeout_micros: u64,
    pub chain_genesis_sha256: String,
    /// Logical CPUs the measuring process observed at launch. In the
    /// identity, not beside the shard count, precisely so the merge's
    /// existing equality check proves every shard saw the SAME host: K
    /// shards spread across two machines would be K measurements of two
    /// different topologies presented as one.
    pub host_logical_cpus: u64,
    /// Total physical memory the measuring process observed at launch, in
    /// bytes. Zero where the platform did not answer.
    pub host_total_memory_bytes: u64,
    /// The corpus's whole harvested population, which the compute-cap
    /// projection costs episode by episode. Carried here because the merge
    /// reads shard reports and nothing else: it recomputes the projection
    /// and so it needs the population, not a summary of it.
    pub all_episode_decisions: TtsS1AllEpisodeDecisionStatsV1,
}

impl TtsS1ReplayIdentityV1 {
    /// The compiled-constant fields, checked rather than trusted.
    ///
    /// A shard produced by a binary whose percentile rule, gating view or
    /// ceiling constants differ is refused at the merge, which is the
    /// same class of guard the launcher's pinned report contract applies
    /// to a finished tier report.
    fn matches_compiled_constants_v1(&self) -> bool {
        self.percentile_rule == TTS_S1_PERCENTILE_RULE_V1
            && self.verdict_view == TTS_S1_VERDICT_VIEW_V1
            && self.slo_micros == slo_micros_v1()
            && self.hard_timeout_micros == hard_timeout_micros_v1()
            && self.chain_genesis_sha256 == TTS_S1_REPLAY_CHAIN_GENESIS_V1
            && self.engine_commit == env!("MTG_KERNEL_BUILD_GIT_HEAD")
    }
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
    /// Stratified targets in the frozen corpus.
    pub corpus_decision_count: u64,
    /// Episodes in the corpus that contribute at least one target.
    pub corpus_episode_count: u64,
    /// Episodes this run replayed end to end.
    pub episodes_replayed: u64,
    /// EVERY decision searched, which is every decision of every replayed
    /// episode, not only the targets. This is the replay's own cost, and
    /// it is on the wire so the cost of a tier is visible.
    pub searched_decisions: u64,
    /// How many of those were stratified corpus targets.
    pub corpus_targets_replayed: u64,
    /// The guard the run was launched under.
    pub max_episodes: u64,
    /// False when `--limit-episodes` cut the run short, or when any corpus
    /// target went unreached. A partial run is a smoke, never a
    /// feasibility verdict a panel may rely on, and the verdict says so.
    pub replayed_whole_corpus: bool,
    pub percentile_rule: String,
    /// Which view the SLO and hard-timeout verdict is taken from, on the
    /// wire so a reader never has to infer it. Always
    /// [`TTS_S1_VERDICT_VIEW_V1`].
    pub verdict_view: String,
    /// THE VERDICT BASIS: the frozen stratified corpus's own 512 targets,
    /// which is the population the sketch defines S1 over. Each one was
    /// searched at its true position in a whole replayed episode, so its
    /// publication cost is a panel's, not a short file's.
    pub corpus_target_view: TtsS1LatencyViewV1,
    /// DIAGNOSTIC: every decision searched, in episode order. It is the
    /// population the compute-cap curve is fitted to, and it is reported
    /// so the corpus targets can be read against the game they came from;
    /// it is NOT the latency gate, because S1's measured population is the
    /// frozen corpus.
    pub whole_episode_view: TtsS1LatencyViewV1,
    pub slo_micros: u64,
    pub hard_timeout_micros: u64,
    /// THE MACHINE THESE TIMINGS WERE TAKEN ON: the concurrency the replay
    /// ran at, the pinned formal one, and the host it ran on. Every latency
    /// in this report is a wall-time sample, so the topology is part of
    /// what the numbers mean and not metadata beside them.
    pub shard_topology: TtsS1ShardTopologyV1,
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
    if body.decisions.len() as u64 != body.searched_decisions
        || body
            .decisions
            .iter()
            .filter(|record| record.is_corpus_target)
            .count() as u64
            != body.corpus_targets_replayed
    {
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
        "whole_episode_view",
        "corpus_target_view",
        "diagnostics_episode_files",
        "compute_cap",
        "verdict",
        "verdict_reason",
        "final_record_sha256",
        // The topology belongs with the timings, not with the substantive
        // claim: it records the machine and the concurrency the wall-time
        // samples were taken under, and it is exactly what changes when the
        // same corpus is replayed by eight processes instead of one. What
        // must NOT change is everything below.
        "shard_topology",
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
    /// An empty corpus, or `--limit-episodes 0`.
    NoDecisions,
    /// The corpus names more contributing episodes than the launcher's
    /// `--max-episodes` guard allows.
    TooManyEpisodes {
        episodes: u64,
        max_episodes: u64,
    },
    /// Playing the recorded episode sequence did not land on a terminal,
    /// so the episode the corpus describes is not the one the kernel just
    /// played.
    EpisodeDidNotTerminate {
        episode_id: u64,
    },
    /// The requested shard owns no episode at all: the fan-out is wider
    /// than the corpus's contributing episode population. The corpus states
    /// that population precisely so a launcher can size the fan-out before
    /// starting, so this is refused rather than published empty.
    EmptyShard {
        shard_index: u64,
        shard_count: u64,
        planned_episodes: u64,
    },
    /// `--shard-index` and `--shard-count` do not name a shard.
    InvalidShardSelector {
        shard_index: u64,
        shard_count: u64,
    },
    /// A published shard report did not re-prove.
    InvalidShardReport,
    /// A shard report could not be read.
    ShardRead(String),
    /// The K shard reports are not one run's: a missing or duplicated
    /// shard, a disagreeing identity, a duplicated or unreplayed episode,
    /// or a stray report from a different fan-out.
    ShardMerge(String),
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
            Self::TooManyEpisodes { .. } => "tts_s1_replay_too_many_episodes",
            Self::EpisodeDidNotTerminate { .. } => "tts_s1_replay_episode_did_not_terminate",
            Self::EmptyShard { .. } => "tts_s1_replay_empty_shard",
            Self::InvalidShardSelector { .. } => "tts_s1_replay_invalid_shard_selector",
            Self::InvalidShardReport => "tts_s1_replay_shard_report_invalid",
            Self::ShardRead(_) => "tts_s1_replay_shard_unreadable",
            Self::ShardMerge(_) => "tts_s1_replay_shard_merge_invalid",
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
            | Self::ShardRead(detail)
            | Self::ShardMerge(detail)
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
            Self::TooManyEpisodes {
                episodes,
                max_episodes,
            } => write!(
                formatter,
                "{}: the corpus contributes {episodes} episodes, above the --max-episodes guard of {max_episodes}",
                self.code_v1()
            ),
            Self::EpisodeDidNotTerminate { episode_id } => {
                write!(formatter, "{}: episode {episode_id}", self.code_v1())
            }
            Self::EmptyShard {
                shard_index,
                shard_count,
                planned_episodes,
            } => write!(
                formatter,
                "{}: shard {shard_index} of {shard_count} owns no episode, because the run plans only {planned_episodes}; size the fan-out from the corpus's contributing_episode_count",
                self.code_v1()
            ),
            Self::InvalidShardSelector {
                shard_index,
                shard_count,
            } => write!(
                formatter,
                "{}: --shard-index {shard_index} --shard-count {shard_count} is not a shard; the count is 1..={TTS_S1_MAX_SHARD_COUNT_V1} and the index is below it",
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
    /// Fail-closed guard on how many whole episodes this replay may run.
    /// Whole-episode replay costs one search per decision of every
    /// contributing episode, so a tier is far more work than the corpus
    /// size suggests; the launcher states the bound it expects and a
    /// corpus above it is refused before anything runs.
    pub max_episodes: u64,
    /// Smoke bound on contributing episodes. `None` replays them all,
    /// which is the only configuration whose verdict a panel may rely on.
    pub limit_episodes: Option<u64>,
    /// Which slice of the planned episodes THIS process replays. `None` is
    /// the unsharded run and publishes a full tier report; `Some` publishes
    /// a shard report for [`merge_tts_s1_replay_shards_v1`]. See
    /// [`TtsS1ShardSelectorV1`]: it changes which process does which
    /// episode and nothing else.
    pub shard: Option<TtsS1ShardSelectorV1>,
    /// Where the PRODUCTION model-guided diagnostics writer publishes this
    /// run's V4 episode files, and where the scorer-shaped response lines
    /// are written. Not optional: the protocol latency the SLO is
    /// classified on is measured BY that writer, so a run without it could
    /// only report the narrower `decision_micros` and would understate
    /// what a panel pays.
    pub diagnostics_directory: PathBuf,
}

/// Reconstructs, searches, times, and reports one tier over one corpus.
///
/// Refuses a sharded configuration: the two publish different documents,
/// so which one is being produced is a decision the caller states rather
/// than one this function makes from a `None`.
pub fn run_tts_s1_replay_v1(
    config: &TtsS1ReplayConfigV1,
) -> Result<TtsS1ReplayReportV1, TtsS1ReplayErrorV1> {
    if let Some(selector) = config.shard {
        return Err(TtsS1ReplayErrorV1::InvalidShardSelector {
            shard_index: selector.shard_index,
            shard_count: selector.shard_count,
        });
    }
    let (corpus, action_seed, loaded, architecture, checkpoint) = load_replay_inputs_v1(config)?;
    let body = replay_corpus_body_v1(
        &loaded.inference,
        &loaded.identity,
        &architecture,
        checkpoint,
        &corpus,
        config.tier,
        config.seed_block_id,
        action_seed,
        config.max_episodes,
        config.limit_episodes,
        &config.diagnostics_directory,
    )?;
    TtsS1ReplayReportV1::seal_v1(body)
}

/// Reconstructs, searches, times, and reports ONE SHARD of one tier.
pub fn run_tts_s1_replay_shard_v1(
    config: &TtsS1ReplayConfigV1,
) -> Result<TtsS1ReplayShardReportV1, TtsS1ReplayErrorV1> {
    let shard = config
        .shard
        .ok_or(TtsS1ReplayErrorV1::InvalidShardSelector {
            shard_index: 0,
            shard_count: 0,
        })?;
    let (corpus, action_seed, loaded, architecture, checkpoint) = load_replay_inputs_v1(config)?;
    let body = replay_corpus_shard_body_v1(
        &loaded.inference,
        &loaded.identity,
        &architecture,
        checkpoint,
        &corpus,
        config.tier,
        config.seed_block_id,
        action_seed,
        config.max_episodes,
        config.limit_episodes,
        shard,
        &config.diagnostics_directory,
    )?;
    TtsS1ReplayShardReportV1::seal_v1(body)
}

/// The corpus, the authorized action seed, and the loaded checkpoint: the
/// inputs both the whole run and one shard of it open with, so the two
/// cannot resolve them differently.
#[allow(clippy::type_complexity)]
fn load_replay_inputs_v1(
    config: &TtsS1ReplayConfigV1,
) -> Result<
    (
        TtsS1CorpusManifestV1,
        u64,
        crate::native_checkpoint_shadow_stdio_v1::LoadedShadowCheckpointV1,
        String,
        TtsS1CorpusCheckpointV1,
    ),
    TtsS1ReplayErrorV1,
> {
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
    Ok((corpus, action_seed, loaded, architecture, checkpoint))
}

/// The replay itself, over the narrow model seam.
///
/// Separated from [`run_tts_s1_replay_v1`] so this crate's own tests can
/// drive the whole reconstruct-verify-search-record path against the
/// in-memory runner-fixed net, with no Store on disk, exactly as the S0
/// search tests do.
///
/// `shard` is `None` for the unsharded run, which replays every planned
/// episode; `Some` restricts the run to that shard's episodes and changes
/// NOTHING else. See [`TtsS1ShardSelectorV1`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn replay_corpus_pass_v1(
    scorer: &dyn TtsS1DecisionScorerV1,
    identity: &ShadowCheckpointIdentityV1,
    architecture: &str,
    checkpoint: TtsS1CorpusCheckpointV1,
    corpus: &TtsS1CorpusManifestV1,
    tier: KernelNativeSearchTierV1,
    seed_block_id: usize,
    action_seed: u64,
    max_episodes: u64,
    limit_episodes: Option<u64>,
    shard: Option<TtsS1ShardSelectorV1>,
    diagnostics_directory: &Path,
) -> Result<TtsS1ReplayPassV1, TtsS1ReplayErrorV1> {
    // Read AT LAUNCH, before a single decision is searched, so the recorded
    // topology is the one the measurements were taken under and not one
    // sampled after the run has already loaded the machine.
    let host = TtsS1HostFactsV1::read_v1();

    // The corpus names the checkpoint it was drawn from; measuring a
    // different one would produce a real report about a population that
    // does not exist.
    if checkpoint != corpus.body.checkpoint {
        return Err(TtsS1ReplayErrorV1::CheckpointMismatch);
    }

    let corpus_decision_count = corpus.body.decisions.len() as u64;
    let corpus_episode_count = corpus.body.episodes.len() as u64;
    if corpus_decision_count == 0 || corpus_episode_count == 0 {
        return Err(TtsS1ReplayErrorV1::NoDecisions);
    }
    // THE GUARD. Whole-episode replay costs one search per decision of
    // every contributing episode, not one per corpus target, so a tier can
    // legitimately be two orders of magnitude more work than the corpus
    // size suggests. The launcher states the bound it expects and the run
    // refuses to start above it, rather than discovering after two days
    // that it was never going to finish.
    if corpus_episode_count > max_episodes {
        return Err(TtsS1ReplayErrorV1::TooManyEpisodes {
            episodes: corpus_episode_count,
            max_episodes,
        });
    }
    let planned_episodes = limit_episodes
        .unwrap_or(corpus_episode_count)
        .min(corpus_episode_count);
    if planned_episodes == 0 {
        return Err(TtsS1ReplayErrorV1::NoDecisions);
    }
    // A shard with no episode at all is refused rather than published
    // empty. It is not a harmless no-op: it costs a process and a loaded
    // checkpoint to measure nothing, and it means the operator asked for a
    // fan-out the corpus cannot supply. The corpus states its contributing
    // episode count precisely so a launcher can size the fan-out before
    // starting, so this is a configuration error the run refuses, not a
    // shape the merge has to tolerate.
    if let Some(selector) = shard {
        if selector.shard_count > planned_episodes {
            return Err(TtsS1ReplayErrorV1::EmptyShard {
                shard_index: selector.shard_index,
                shard_count: selector.shard_count,
                planned_episodes,
            });
        }
    }

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
    let mut corpus_targets_replayed: u64 = 0;
    // One block per episode this pass owns, in the corpus's own episode
    // order, carrying the position the merge reassembles by.
    let mut pass_episodes: Vec<TtsS1ShardEpisodeV1> = Vec::new();

    // WHOLE EPISODES, in the corpus's own episode order. Every decision of
    // a contributing episode is reconstructed and searched, in order, not
    // only the stratified targets. That is not thoroughness for its own
    // sake: the production writer republishes the whole episode file after
    // every decision, so a decision's publication cost is a function of
    // every earlier searched decision in that episode. A replay that
    // searched only the sparse targets would publish short files and
    // measure a publication phase no panel ever pays.
    //
    // A SHARD skips the episodes it does not own, and skips them entirely:
    // it never resets, never searches and never publishes for them, which
    // is the whole of what parallelism buys here.
    for (position, episode) in corpus
        .body
        .episodes
        .iter()
        .enumerate()
        .take(usize::try_from(planned_episodes).map_err(|_| TtsS1ReplayErrorV1::NoDecisions)?)
    {
        let position = position as u64;
        if let Some(selector) = shard {
            if !selector.owns_position_v1(position) {
                continue;
            }
        }
        let episode_started = Instant::now();
        let episode_first_record_index = records.len() as u64;
        let mut episode_corpus_targets: u64 = 0;
        let (mut session, schedule) = reset_episode_v1(
            episode,
            corpus.body.max_physical_decisions,
            corpus.body.max_policy_steps,
        )?;
        // This episode's targets, ascending. The decision list is already
        // in ascending (episode id, ordinal) order, which `decode` proved.
        let mut targets = corpus
            .body
            .decisions
            .iter()
            .filter(|decision| decision.coordinates.episode_id == episode.episode_id)
            .peekable();
        let mut episode_record_indices: Vec<usize> = Vec::new();
        let mut episode_opened = false;

        for ordinal in 0..episode.decision_count {
            let expected = match session.current_response() {
                FastActorResponseV1::Decision(decision) => decision,
                FastActorResponseV1::Terminal(_) => {
                    return Err(TtsS1ReplayErrorV1::ReconstructionMismatch {
                        episode_id: episode.episode_id,
                        decision_ordinal: ordinal,
                        field: "early_terminal",
                    })
                }
            };

            // A target at this ordinal gets the full fail-closed surface
            // check. The decisions between targets are still searched and
            // still published, because they are what gives the targets
            // their real publication history, but the corpus makes no
            // claim about them to check against.
            let mut is_corpus_target = false;
            if targets
                .peek()
                .is_some_and(|target| target.coordinates.decision_ordinal == ordinal)
            {
                let target = targets.next().ok_or(TtsS1ReplayErrorV1::CorpusOrder)?;
                verify_reconstructed_surface_v1(&session, expected, target)?;
                is_corpus_target = true;
                corpus_targets_replayed += 1;
                episode_corpus_targets += 1;
            }

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
                        episode.episode_id,
                        episode.episode_base_seed,
                        schedule.learner_seat,
                        bound_ref.wrapper_identity.clone(),
                    )
                    .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
                episode_opened = true;
            }

            let context = TtsS1DecisionContextV1 {
                identity,
                deck_ids: &corpus.body.deck_ids,
                base_seed: episode.episode_base_seed,
                schedule: &schedule,
            };
            let record = search_and_publish_one_decision_v1(
                scorer,
                bound_ref,
                &value_domain,
                &session,
                expected,
                &context,
                episode.episode_id,
                ordinal,
                is_corpus_target,
                &mut diagnostics,
                &mut responses,
            )?;
            episode_record_indices.push(records.len());
            records.push(record);

            let action = episode
                .action_sequence
                .get(usize::try_from(ordinal).unwrap_or(usize::MAX))
                .copied()
                .ok_or(TtsS1ReplayErrorV1::CorpusOrder)?;
            session
                .consume_current_flat_action_slice_v2(
                    session
                        .native_full_trajectory_current_binding_v2(expected)
                        .map_err(|error| TtsS1ReplayErrorV1::Binding(format!("{error:?}")))?,
                    action,
                )
                .map_err(|error| TtsS1ReplayErrorV1::Consume(format!("{:?}", error.code)))?;
        }

        // Playing the whole recorded sequence must land on a terminal. If
        // it does not, the episode the corpus describes is not the episode
        // the kernel just played, and every latency measured from it would
        // be a measurement of something else.
        if !matches!(session.current_response(), FastActorResponseV1::Terminal(_)) {
            return Err(TtsS1ReplayErrorV1::EpisodeDidNotTerminate {
                episode_id: episode.episode_id,
            });
        }
        if targets.next().is_some() {
            return Err(TtsS1ReplayErrorV1::CorpusOrder);
        }

        if episode_opened {
            let path = diagnostics.episode_path_v4(episode.episode_id, episode.episode_base_seed);
            // The episode was played to its terminal, so the honest close
            // reason is the terminal one, and the footer is what gives the
            // episode's LAST decision a successor and therefore a protocol
            // verdict at all.
            diagnostics
                .close_episode_v4(EpisodeCloseReasonV4::EpisodeTerminal)
                .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
            pass_episodes.push(TtsS1ShardEpisodeV1 {
                episode_position: position,
                episode_id: episode.episode_id,
                episode_base_seed: episode.episode_base_seed,
                decision_count: episode.decision_count,
                searched_decisions: episode_record_indices.len() as u64,
                corpus_targets_replayed: episode_corpus_targets,
                first_record_index: episode_first_record_index,
                elapsed_micros: elapsed_micros_v1(episode_started),
                // Backfilled once the writer-observed phases are read off
                // the published file, below: a protocol total summed from
                // records that do not yet carry one would be a total of
                // zeroes.
                protocol_micros_total: 0,
            });
            published_episodes.push((episode.episode_id, path, episode_record_indices));
        }
    }

    responses
        .flush()
        .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
    drop(responses);

    if records.is_empty() {
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

    // The per-episode protocol totals, now that every record carries one.
    for episode in pass_episodes.iter_mut() {
        let first = usize::try_from(episode.first_record_index)
            .map_err(|_| TtsS1ReplayErrorV1::NoDecisions)?;
        let count = usize::try_from(episode.searched_decisions)
            .map_err(|_| TtsS1ReplayErrorV1::NoDecisions)?;
        let slice = records
            .get(first..first + count)
            .ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
        episode.protocol_micros_total = slice.iter().fold(0u64, |running, record| {
            running.saturating_add(record.wall_time.protocol_micros)
        });
    }

    let bound_ref = bound.as_ref().ok_or(TtsS1ReplayErrorV1::Search(
        "model_guided_search_authority_unbound",
    ))?;
    Ok(TtsS1ReplayPassV1 {
        identity: TtsS1ReplayIdentityV1 {
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
            corpus_episode_count,
            episodes_replayed: planned_episodes,
            max_episodes,
            percentile_rule: TTS_S1_PERCENTILE_RULE_V1.to_owned(),
            verdict_view: TTS_S1_VERDICT_VIEW_V1.to_owned(),
            slo_micros: slo_micros_v1(),
            hard_timeout_micros: hard_timeout_micros_v1(),
            chain_genesis_sha256: TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned(),
            host_logical_cpus: host.logical_cpus,
            host_total_memory_bytes: host.total_memory_bytes,
            all_episode_decisions: corpus.body.all_episode_decisions.clone(),
        },
        records,
        diagnostics_episode_files,
        episodes: pass_episodes,
        corpus_targets_replayed,
    })
}

/// What one replay process produced, before any statistic is computed over
/// it. The unsharded path finalizes it into a full report; a shard
/// publishes it for the merge to finalize the union.
pub(crate) struct TtsS1ReplayPassV1 {
    pub(crate) identity: TtsS1ReplayIdentityV1,
    /// Every searched decision, in episode order, with the writer-observed
    /// phases already backfilled and the chain NOT yet assigned.
    pub(crate) records: Vec<TtsS1ReplayDecisionRecordV1>,
    pub(crate) diagnostics_episode_files: Vec<TtsS1DiagnosticsEpisodeFileV1>,
    pub(crate) episodes: Vec<TtsS1ShardEpisodeV1>,
    pub(crate) corpus_targets_replayed: u64,
}

/// Turns a complete set of records into the published report body.
///
/// THE SINGLE SITE where every published statistic is computed. The
/// unsharded replay calls it over its own records and the merge calls it
/// over the union of the shards', so "the merged report is what an
/// unsharded replay would have published" is true by construction rather
/// than by two implementations agreeing. Nothing here can see how many
/// processes produced `records`, and nothing here reads a clock.
///
/// `records` must already be in the unsharded order: episodes in the
/// corpus's own order, decisions ascending within each episode. The chain
/// is assigned here, over that order.
pub(crate) fn finalize_tts_s1_replay_body_v1(
    identity: &TtsS1ReplayIdentityV1,
    mut records: Vec<TtsS1ReplayDecisionRecordV1>,
    diagnostics_episode_files: Vec<TtsS1DiagnosticsEpisodeFileV1>,
    corpus_targets_replayed: u64,
    shard_count: u64,
) -> Result<TtsS1ReplayReportBodyV1, TtsS1ReplayErrorV1> {
    let searched_decisions = records.len() as u64;
    if searched_decisions == 0 {
        return Err(TtsS1ReplayErrorV1::NoDecisions);
    }

    // The chain is assigned LAST, over finished records, because the
    // writer-observed phases are part of what it commits to.
    let mut previous_record_sha256 = TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned();
    for (ordinal, record) in records.iter_mut().enumerate() {
        record.record_ordinal = ordinal as u64;
        record.previous_record_sha256 = previous_record_sha256.clone();
        previous_record_sha256 = lower_hex_sha256_v4(record.chain_link_v1()?);
    }

    // The two views. The whole-episode one is every searched decision; the
    // corpus-target one is the stratified subset and is the verdict basis.
    let all_records: Vec<&TtsS1ReplayDecisionRecordV1> = records.iter().collect();
    let target_records: Vec<&TtsS1ReplayDecisionRecordV1> = records
        .iter()
        .filter(|record| record.is_corpus_target)
        .collect();
    let whole_episode_view =
        latency_view_v1(&all_records).ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
    let corpus_target_view =
        latency_view_v1(&target_records).ok_or(TtsS1ReplayErrorV1::NoDecisions)?;

    // The curve is fitted to the WHOLE-EPISODE population, because that is
    // where per-ordinal publication growth is observable at all; the
    // stratified targets are a sparse sample of ordinals and could not fit
    // one. The latency VERDICT is a separate question and is taken from
    // the corpus targets, which is the population the sketch defines.
    let curve_samples: Vec<(u64, u64)> = records
        .iter()
        .map(|record| (record.decision_ordinal, record.wall_time.protocol_micros))
        .collect();
    let compute_cap = compute_cap_projection_v1(&curve_samples, &identity.all_episode_decisions)
        .ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
    let replayed_whole_corpus = identity.episodes_replayed == identity.corpus_episode_count
        && corpus_targets_replayed == identity.corpus_decision_count;
    let (verdict, verdict_reason) = verdict_v1(
        corpus_target_view.p99_protocol_ceiling_status,
        corpus_target_view.max_protocol_ceiling_status,
        compute_cap.within_cap,
        replayed_whole_corpus,
    );

    let body = TtsS1ReplayReportBodyV1 {
        engine_commit: identity.engine_commit.clone(),
        tier: identity.tier.clone(),
        transition_budget: identity.transition_budget,
        policy_step_depth_cap: identity.policy_step_depth_cap,
        seed_block_id: identity.seed_block_id,
        seed_block_seed: identity.seed_block_seed,
        stability_halves_enabled: identity.stability_halves_enabled,
        checkpoint: identity.checkpoint.clone(),
        wrapper_identity: identity.wrapper_identity.clone(),
        search_authority_digest_sha256: identity.search_authority_digest_sha256.clone(),
        corpus_sha256: identity.corpus_sha256.clone(),
        corpus_decision_count: identity.corpus_decision_count,
        corpus_episode_count: identity.corpus_episode_count,
        episodes_replayed: identity.episodes_replayed,
        searched_decisions,
        corpus_targets_replayed,
        max_episodes: identity.max_episodes,
        replayed_whole_corpus,
        percentile_rule: TTS_S1_PERCENTILE_RULE_V1.to_owned(),
        verdict_view: TTS_S1_VERDICT_VIEW_V1.to_owned(),
        corpus_target_view,
        whole_episode_view,
        slo_micros: slo_micros_v1(),
        hard_timeout_micros: hard_timeout_micros_v1(),
        shard_topology: TtsS1ShardTopologyV1::evaluate_v1(
            shard_count,
            TtsS1HostFactsV1 {
                logical_cpus: identity.host_logical_cpus,
                total_memory_bytes: identity.host_total_memory_bytes,
            },
        ),
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

/// The unsharded replay: every planned episode in one process.
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
    max_episodes: u64,
    limit_episodes: Option<u64>,
    diagnostics_directory: &Path,
) -> Result<TtsS1ReplayReportBodyV1, TtsS1ReplayErrorV1> {
    let pass = replay_corpus_pass_v1(
        scorer,
        identity,
        architecture,
        checkpoint,
        corpus,
        tier,
        seed_block_id,
        action_seed,
        max_episodes,
        limit_episodes,
        None,
        diagnostics_directory,
    )?;
    finalize_tts_s1_replay_body_v1(
        &pass.identity,
        pass.records,
        pass.diagnostics_episode_files,
        pass.corpus_targets_replayed,
        // ONE process did all of it, which is the topology this report is
        // entitled to claim and, being one and not eight, is never formal.
        1,
    )
}

/// One shard's replay: its own episodes, its own chain, and no verdict.
#[allow(clippy::too_many_arguments)]
pub(crate) fn replay_corpus_shard_body_v1(
    scorer: &dyn TtsS1DecisionScorerV1,
    identity: &ShadowCheckpointIdentityV1,
    architecture: &str,
    checkpoint: TtsS1CorpusCheckpointV1,
    corpus: &TtsS1CorpusManifestV1,
    tier: KernelNativeSearchTierV1,
    seed_block_id: usize,
    action_seed: u64,
    max_episodes: u64,
    limit_episodes: Option<u64>,
    shard: TtsS1ShardSelectorV1,
    diagnostics_directory: &Path,
) -> Result<TtsS1ReplayShardReportBodyV1, TtsS1ReplayErrorV1> {
    let mut pass = replay_corpus_pass_v1(
        scorer,
        identity,
        architecture,
        checkpoint,
        corpus,
        tier,
        seed_block_id,
        action_seed,
        max_episodes,
        limit_episodes,
        Some(shard),
        diagnostics_directory,
    )?;
    if pass.records.is_empty() || pass.episodes.is_empty() {
        return Err(TtsS1ReplayErrorV1::NoDecisions);
    }
    // The shard chains its OWN records, so a shard report is verifiable on
    // its own before anything is merged. The merge re-chains over the
    // union, because the published chain is over the whole run's records in
    // the whole run's order and a shard cannot know that order.
    let mut previous_record_sha256 = TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned();
    for (ordinal, record) in pass.records.iter_mut().enumerate() {
        record.record_ordinal = ordinal as u64;
        record.previous_record_sha256 = previous_record_sha256.clone();
        previous_record_sha256 = lower_hex_sha256_v4(record.chain_link_v1()?);
    }
    let body = TtsS1ReplayShardReportBodyV1 {
        identity: pass.identity,
        shard_assignment_rule: TTS_S1_SHARD_ASSIGNMENT_RULE_V1.to_owned(),
        shard_index: shard.shard_index,
        shard_count: shard.shard_count,
        shard_episodes_replayed: pass.episodes.len() as u64,
        searched_decisions: pass.records.len() as u64,
        corpus_targets_replayed: pass.corpus_targets_replayed,
        episodes: pass.episodes,
        diagnostics_episode_files: pass.diagnostics_episode_files,
        chain_genesis_sha256: TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned(),
        final_record_sha256: previous_record_sha256,
        decisions: pass.records,
    };
    verify_tts_s1_replay_shard_body_v1(&body)?;
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
/// 2. p99 PROTOCOL wall time OVER THE CORPUS TARGETS is not inside the
///    4.0 s SLO (Section 4). The corpus is the population the sketch
///    defines S1 over; see [`TTS_S1_VERDICT_VIEW_V1`].
/// 3. Any corpus target reached the 20.0 s hard protocol timeout, which
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
            "p99 corpus-target protocol wall time exceeds the {MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4} s SLO"
        ));
    }
    if max == CeilingStatusV4::HardTimeoutExceeded {
        failures.push(format!(
            "a corpus target reached the {MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4} s hard protocol timeout"
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
                "the whole corpus replayed; p99 corpus-target protocol wall time is inside the {MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4} s SLO, no corpus target reached the {MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4} s hard protocol timeout, and the projected S2 cost is inside the {} worker-hour compute cap",
                TTS_S1_S2_COMPUTE_CAP_WORKER_HOURS_MILLI_V1 / 1_000
            ),
        );
    }
    (TtsS1TierVerdictV1::Infeasible, failures.join("; "))
}

fn reset_episode_v1(
    episode: &TtsS1CorpusEpisodeV1,
    max_physical_decisions: u64,
    max_policy_steps: u64,
) -> Result<(FastActorSessionV1, NativeTrainerEpisodeScheduleV1), TtsS1ReplayErrorV1> {
    let schedule =
        native_trainer_episode_schedule_v1(episode.episode_base_seed, episode.episode_id)
            .map_err(|_| TtsS1ReplayErrorV1::EpisodeSchedule)?;
    if schedule.environment_seed != episode.environment_seed {
        return Err(TtsS1ReplayErrorV1::ReconstructionMismatch {
            episode_id: episode.episode_id,
            decision_ordinal: 0,
            field: "environment_seed",
        });
    }
    let session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
        episode.episode_id,
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
    context: &TtsS1DecisionContextV1<'_>,
    episode_id: u64,
    decision_ordinal: u64,
    is_corpus_target: bool,
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
    let action_seed = corpus_policy_sample_seed_v1(context.base_seed, episode_id, expected);
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
        request_id: format!("tts-s1-{episode_id}-{decision_ordinal}"),
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
        episode_id,
        decision_ordinal,
        is_corpus_target,
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

// ---------------------------------------------------------------------------
// SHARDING. One tier's replay, split across K processes by episode, and the
// merge that recomputes the tier report over the union.
// ---------------------------------------------------------------------------

/// One episode a shard replayed, with the position that reassembles it.
///
/// `elapsed_micros` and `protocol_micros_total` are WALL TIME and are
/// deliberately shard-report-only: neither reaches the merged tier report,
/// which computes every published statistic from the decision records
/// themselves. They are here because a launcher that has just paid for K
/// processes should be able to say what each episode cost it, and because
/// the next tier's expected cost is read off exactly these numbers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ShardEpisodeV1 {
    /// Position in the corpus's own episode order. THE reassembly key: the
    /// merged report's records are in position order, which is the order an
    /// unsharded replay would have produced them in.
    pub episode_position: u64,
    pub episode_id: u64,
    pub episode_base_seed: u64,
    /// Decisions the corpus recorded for this episode.
    pub decision_count: u64,
    /// Decisions this shard searched in it. Equal to `decision_count`,
    /// because episodes are replayed whole or not at all.
    pub searched_decisions: u64,
    pub corpus_targets_replayed: u64,
    /// Where this episode's records start inside this shard's own record
    /// list, so the merge can cut them out without re-scanning.
    pub first_record_index: u64,
    /// WALL TIME. Shard-report only.
    pub elapsed_micros: u64,
    /// The episode's summed protocol latency. WALL TIME, shard-report only.
    pub protocol_micros_total: u64,
}

/// Everything one shard's report digest covers.
///
/// What is NOT here is the point: no verdict, no verdict reason, no
/// latency view, no compute-cap projection. A fraction of the episodes can
/// produce none of them honestly, so a shard report does not carry a field
/// for them at all rather than carrying a partial one somebody might read.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ReplayShardReportBodyV1 {
    /// Shared by every shard of this run, and checked to be.
    pub identity: TtsS1ReplayIdentityV1,
    pub shard_assignment_rule: String,
    pub shard_index: u64,
    pub shard_count: u64,
    /// Episodes THIS shard replayed.
    pub shard_episodes_replayed: u64,
    /// Decisions THIS shard searched.
    pub searched_decisions: u64,
    /// Stratified corpus targets THIS shard reached.
    pub corpus_targets_replayed: u64,
    pub episodes: Vec<TtsS1ShardEpisodeV1>,
    /// The V4 diagnostics episode files this shard published, in the same
    /// order as `episodes`, so the merged report commits to every artifact
    /// its protocol latencies were read from whichever process wrote it.
    pub diagnostics_episode_files: Vec<TtsS1DiagnosticsEpisodeFileV1>,
    pub chain_genesis_sha256: String,
    /// SHA-256 of this shard's last record, over the shard's own chain.
    pub final_record_sha256: String,
    pub decisions: Vec<TtsS1ReplayDecisionRecordV1>,
}

/// The published shard report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ReplayShardReportV1 {
    pub schema: String,
    /// SHA-256 over the canonical JSON bytes of `body`, lower hex.
    pub shard_report_sha256: String,
    pub body: TtsS1ReplayShardReportBodyV1,
}

impl TtsS1ReplayShardReportV1 {
    pub fn seal_v1(body: TtsS1ReplayShardReportBodyV1) -> Result<Self, TtsS1ReplayErrorV1> {
        let bytes = to_canonical_json_bytes_v1(&body, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1ReplayErrorV1::CanonicalJson)?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest: [u8; 32] = hasher.finalize().into();
        Ok(Self {
            schema: TTS_S1_REPLAY_SHARD_REPORT_SCHEMA_V1.to_owned(),
            shard_report_sha256: lower_hex_sha256_v4(digest),
            body,
        })
    }

    pub fn canonical_bytes_v1(&self) -> Result<Vec<u8>, TtsS1ReplayErrorV1> {
        to_canonical_json_bytes_v1(self, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1ReplayErrorV1::CanonicalJson)
    }
}

/// The deterministic file name one shard publishes under.
///
/// The merge is given a DIRECTORY and a shard count and nothing else, so
/// the names have to be derivable from those two numbers: a merge that
/// globbed for whatever it found could silently merge three shards of a
/// four-shard run, which is the exact failure the whole design has to make
/// impossible.
pub fn tts_s1_shard_report_file_name_v1(shard_index: u64, shard_count: u64) -> String {
    format!("shard-{shard_index:04}-of-{shard_count:04}.report.json")
}

/// Whether a file name is one of these reports at all, used to reject a
/// directory holding a LEFTOVER shard set from a different fan-out.
fn is_tts_s1_shard_report_file_name_v1(name: &str) -> bool {
    name.starts_with("shard-") && name.ends_with(".report.json")
}

/// Re-proves a published shard report: canonical bytes, schema, own digest,
/// and every internal invariant.
pub fn decode_tts_s1_replay_shard_report_v1(
    bytes: &[u8],
) -> Result<TtsS1ReplayShardReportV1, TtsS1ReplayErrorV1> {
    let report: TtsS1ReplayShardReportV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1ReplayErrorV1::InvalidShardReport)?;
    let reencoded = to_canonical_json_bytes_v1(&report, CanonicalJsonNullPolicyV1::Forbid)
        .map_err(|_| TtsS1ReplayErrorV1::CanonicalJson)?;
    let resealed = TtsS1ReplayShardReportV1::seal_v1(report.body.clone())?;
    if reencoded != bytes
        || report.schema != TTS_S1_REPLAY_SHARD_REPORT_SCHEMA_V1
        || resealed.shard_report_sha256 != report.shard_report_sha256
    {
        return Err(TtsS1ReplayErrorV1::InvalidShardReport);
    }
    verify_tts_s1_replay_shard_body_v1(&report.body)?;
    Ok(report)
}

/// Publishes a shard report atomically and immutably.
pub fn publish_tts_s1_replay_shard_report_v1(
    report: &TtsS1ReplayShardReportV1,
    path: &Path,
) -> Result<Vec<u8>, TtsS1ReplayErrorV1> {
    let bytes = report.canonical_bytes_v1()?;
    publish_canonical_document_v1(&bytes, path)
        .map_err(|error| TtsS1ReplayErrorV1::Publication(error.to_string()))?;
    Ok(bytes)
}

/// Every invariant a shard report must satisfy on its own, before any
/// question of merging arises.
///
/// It is deliberately exhaustive rather than "enough to reassemble":
/// the merge trusts these, so an unchecked one would be an assumption the
/// merged report inherits silently.
pub fn verify_tts_s1_replay_shard_body_v1(
    body: &TtsS1ReplayShardReportBodyV1,
) -> Result<(), TtsS1ReplayErrorV1> {
    let invalid = |detail: &str| TtsS1ReplayErrorV1::ShardMerge(detail.to_owned());
    let selector = TtsS1ShardSelectorV1::new_v1(body.shard_index, body.shard_count)
        .ok_or_else(|| invalid("shard index or shard count out of range"))?;
    if body.shard_assignment_rule != TTS_S1_SHARD_ASSIGNMENT_RULE_V1 {
        return Err(invalid("shard declares a different assignment rule"));
    }
    if body.chain_genesis_sha256 != TTS_S1_REPLAY_CHAIN_GENESIS_V1 {
        return Err(TtsS1ReplayErrorV1::BrokenChain);
    }
    // The shard's own chain, exactly as the full report's is walked.
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
    if body.decisions.len() as u64 != body.searched_decisions
        || body
            .decisions
            .iter()
            .filter(|record| record.is_corpus_target)
            .count() as u64
            != body.corpus_targets_replayed
    {
        return Err(TtsS1ReplayErrorV1::BrokenChain);
    }
    if body.episodes.is_empty() || body.episodes.len() as u64 != body.shard_episodes_replayed {
        return Err(invalid(
            "shard episode count disagrees with its episode list",
        ));
    }
    if body.diagnostics_episode_files.len() != body.episodes.len() {
        return Err(invalid(
            "shard published a different number of diagnostics episode files than episodes",
        ));
    }
    // The episodes partition the shard's records exactly, in order, and
    // every one of them is an episode this shard is the owner of.
    let mut cursor = 0u64;
    let mut previous_position: Option<u64> = None;
    let mut searched = 0u64;
    let mut targets = 0u64;
    for (episode, file) in body
        .episodes
        .iter()
        .zip(body.diagnostics_episode_files.iter())
    {
        if !selector.owns_position_v1(episode.episode_position) {
            return Err(invalid(
                "shard carries an episode at a position it does not own",
            ));
        }
        if episode.episode_position >= body.identity.episodes_replayed {
            return Err(invalid("shard carries an episode past the planned run"));
        }
        if previous_position.is_some_and(|previous| previous >= episode.episode_position) {
            return Err(invalid(
                "shard episodes are not in ascending position order",
            ));
        }
        previous_position = Some(episode.episode_position);
        if episode.first_record_index != cursor
            || episode.searched_decisions != episode.decision_count
        {
            return Err(invalid("shard episode records do not partition the shard"));
        }
        let first = usize::try_from(cursor).map_err(|_| invalid("shard record index overflow"))?;
        let count = usize::try_from(episode.searched_decisions)
            .map_err(|_| invalid("shard record index overflow"))?;
        let slice = body
            .decisions
            .get(first..first + count)
            .ok_or_else(|| invalid("shard episode names records it does not carry"))?;
        if slice
            .iter()
            .any(|record| record.episode_id != episode.episode_id)
        {
            return Err(invalid(
                "shard episode records carry a different episode id",
            ));
        }
        if slice
            .iter()
            .filter(|record| record.is_corpus_target)
            .count() as u64
            != episode.corpus_targets_replayed
        {
            return Err(invalid(
                "shard episode target count disagrees with its records",
            ));
        }
        if file.episode_id != episode.episode_id
            || file.decision_record_count != episode.searched_decisions
        {
            return Err(invalid(
                "shard diagnostics episode file does not match its episode",
            ));
        }
        cursor = cursor.saturating_add(episode.searched_decisions);
        searched = searched.saturating_add(episode.searched_decisions);
        targets = targets.saturating_add(episode.corpus_targets_replayed);
    }
    if searched != body.searched_decisions || targets != body.corpus_targets_replayed {
        return Err(invalid(
            "shard episode totals disagree with the shard totals",
        ));
    }
    Ok(())
}

/// Reads exactly `shard_count` shard reports out of `directory` and
/// recomputes the whole tier report over their union.
///
/// # Why this is the same report, not a similar one
///
/// Every published statistic is computed by
/// [`finalize_tts_s1_replay_body_v1`], the SAME function the unsharded
/// replay finalizes through, over records placed in the SAME order (the
/// corpus's episode order, decisions ascending within an episode). The
/// isotonic fit is pooled over every shard's samples because it is fitted
/// to the concatenated record set, not to per-shard fits combined; the two
/// views, the per-episode cost estimates, the projection and the verdict
/// are likewise functions of that one record set. So the merged report
/// differs from an unsharded one only where a re-run of the unsharded
/// replay would differ from itself: in the measured timings and in what
/// they derive.
///
/// # What is refused
///
/// Everything that could make the union not be the run. A missing shard, a
/// duplicated shard index, a shard from a different corpus, tier, seed
/// block, checkpoint, wrapper identity or engine build, a shard whose
/// episodes are not the ones its index owns, a duplicated episode, an
/// episode nobody replayed, or a stray shard report in the directory from
/// some other fan-out. None of them is a warning and none of them is
/// dropped: a partial union would publish a real-looking verdict over a
/// population that was never measured.
pub fn merge_tts_s1_replay_shards_v1(
    directory: &Path,
    shard_count: u64,
) -> Result<TtsS1ReplayReportV1, TtsS1ReplayErrorV1> {
    let invalid = |detail: String| TtsS1ReplayErrorV1::ShardMerge(detail);
    if shard_count == 0 || shard_count > TTS_S1_MAX_SHARD_COUNT_V1 {
        return Err(invalid(format!(
            "shard count {shard_count} is outside 1..={TTS_S1_MAX_SHARD_COUNT_V1}"
        )));
    }
    // The directory must hold EXACTLY this fan-out's reports and no other:
    // a leftover set from a different shard count sitting beside them is
    // how three shards of a four-shard run get merged as if complete.
    let expected: Vec<String> = (0..shard_count)
        .map(|index| tts_s1_shard_report_file_name_v1(index, shard_count))
        .collect();
    let mut observed: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(directory)
        .map_err(|error| TtsS1ReplayErrorV1::ShardRead(error.to_string()))?
    {
        let entry = entry.map_err(|error| TtsS1ReplayErrorV1::ShardRead(error.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_tts_s1_shard_report_file_name_v1(&name) {
            observed.push(name);
        }
    }
    observed.sort();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort();
    if observed != expected_sorted {
        return Err(invalid(format!(
            "{} holds shard reports {:?}, not exactly the {shard_count} this merge requires ({:?})",
            directory.display(),
            observed,
            expected_sorted
        )));
    }

    let mut shards: Vec<TtsS1ReplayShardReportV1> = Vec::with_capacity(expected.len());
    for (index, name) in expected.iter().enumerate() {
        let path = directory.join(name);
        let bytes = std::fs::read(&path)
            .map_err(|error| TtsS1ReplayErrorV1::ShardRead(error.to_string()))?;
        let report = decode_tts_s1_replay_shard_report_v1(&bytes)?;
        if report.body.shard_index != index as u64 || report.body.shard_count != shard_count {
            return Err(invalid(format!(
                "{name} declares shard {} of {}, not shard {index} of {shard_count}",
                report.body.shard_index, report.body.shard_count
            )));
        }
        shards.push(report);
    }

    // ONE identity, shared. The first shard's is the reference and every
    // other must equal it exactly; it must also equal this binary's own
    // compiled constants, so a merge by a differently-built binary is
    // refused rather than silently recomputing under different rules.
    let identity = shards
        .first()
        .map(|shard| shard.body.identity.clone())
        .ok_or_else(|| invalid("no shard reports".to_owned()))?;
    if !identity.matches_compiled_constants_v1() {
        return Err(invalid(
            "the shard reports were produced by a build whose engine commit or pinned \
             constants differ from this one's"
                .to_owned(),
        ));
    }
    for shard in &shards {
        if shard.body.identity != identity {
            return Err(invalid(format!(
                "shard {} was replayed against a different corpus, tier, seed block, \
                 checkpoint or build than shard 0",
                shard.body.shard_index
            )));
        }
    }

    // The episodes across every shard must be exactly the planned run:
    // each position once, no gap, no duplicate, no episode id twice.
    let mut placed: Vec<(u64, usize, usize)> = Vec::new();
    for (slot, shard) in shards.iter().enumerate() {
        for (episode_index, episode) in shard.body.episodes.iter().enumerate() {
            placed.push((episode.episode_position, slot, episode_index));
        }
    }
    placed.sort_unstable_by_key(|(position, _, _)| *position);
    if placed.len() as u64 != identity.episodes_replayed {
        return Err(invalid(format!(
            "the shards replayed {} episodes, not the {} the run planned",
            placed.len(),
            identity.episodes_replayed
        )));
    }
    for (expected_position, (position, _, _)) in placed.iter().enumerate() {
        if *position != expected_position as u64 {
            return Err(invalid(format!(
                "episode position {expected_position} is missing or duplicated across the shards"
            )));
        }
    }
    let mut episode_ids: Vec<u64> = placed
        .iter()
        .map(|(_, slot, episode_index)| shards[*slot].body.episodes[*episode_index].episode_id)
        .collect();
    let placed_count = episode_ids.len();
    episode_ids.sort_unstable();
    episode_ids.dedup();
    if episode_ids.len() != placed_count {
        return Err(invalid(
            "two shards replayed the same episode id".to_owned(),
        ));
    }

    // The union, in the order an unsharded replay would have produced it.
    let mut records: Vec<TtsS1ReplayDecisionRecordV1> = Vec::new();
    let mut diagnostics_episode_files: Vec<TtsS1DiagnosticsEpisodeFileV1> = Vec::new();
    let mut corpus_targets_replayed = 0u64;
    for (_, slot, episode_index) in &placed {
        let shard = &shards[*slot];
        let episode = &shard.body.episodes[*episode_index];
        let first = usize::try_from(episode.first_record_index)
            .map_err(|_| invalid("shard record index overflow".to_owned()))?;
        let count = usize::try_from(episode.searched_decisions)
            .map_err(|_| invalid("shard record index overflow".to_owned()))?;
        let slice = shard
            .body
            .decisions
            .get(first..first + count)
            .ok_or_else(|| invalid("shard episode names records it does not carry".to_owned()))?;
        records.extend(slice.iter().cloned());
        diagnostics_episode_files
            .push(shard.body.diagnostics_episode_files[*episode_index].clone());
        corpus_targets_replayed =
            corpus_targets_replayed.saturating_add(episode.corpus_targets_replayed);
    }

    let body = finalize_tts_s1_replay_body_v1(
        &identity,
        records,
        diagnostics_episode_files,
        corpus_targets_replayed,
        shard_count,
    )?;
    TtsS1ReplayReportV1::seal_v1(body)
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
        corpus_body_v1, harvest_episode_v1, TtsS1AllEpisodeDecisionStatsV1, TtsS1CorpusSelectionV1,
        TtsS1EpisodeDecisionStatsV1, TtsS1FlatScoreV1,
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
    /// The scorer's own caps, deliberately.
    ///
    /// A smaller cap would make the fixture episode cheap, and it was
    /// tried: it does not work, and the reason is worth recording. The
    /// searcher's own simulations run up to the depth cap from whatever
    /// decision they start at, so with a small decision cap a search near
    /// the end of the episode reaches the cap inside its own tree and hits
    /// the `TerminalClassificationV1::Truncated` synthetic-key dispatch,
    /// where the real-forward evaluator fails closed with
    /// `NoLiveDecisionToEncode` by design (see `model_guided_search_core_v1`,
    /// "Determination: the real-forward evaluator's live-decision
    /// requirement"). A short episode therefore cannot be searched at all,
    /// under production caps or any other. The fixture episode is a real
    /// 274-decision game, which is why the whole-episode tests below are
    /// `#[ignore]`d in a debug build.
    const FIXTURE_MAX_PHYSICAL_DECISIONS_V1: u64 = 1_024;
    const FIXTURE_MAX_POLICY_STEPS_V1: u64 = 2_048;
    const FIXTURE_MAX_EPISODES_V1: u64 = 4;

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

    /// Harvests one short seeded self-play episode and seals a corpus over
    /// a strided sample of its decisions, plus the WHOLE episode.
    ///
    /// It bypasses `select_tts_s1_corpus_v1` deliberately: the quota rules
    /// are covered exhaustively by the corpus module's own pure tests, and
    /// filling a 512-decision quota here would mean running thousands of
    /// searches inside a unit test. What this fixture exists to exercise
    /// is the other half, the whole-episode
    /// reconstruct-verify-search-publish path.
    fn fixture_corpus_v1(scorer: &RunnerFixedScorerV1, take: usize) -> TtsS1CorpusManifestV1 {
        fixture_corpus_episodes_v1(scorer, 1, take)
    }

    /// The same fixture over `episode_count` CONTRIBUTING episodes.
    ///
    /// Seeded self-play is walked from episode id 0 upward until that many
    /// NATURAL terminals are found; a truncated one contributes to the
    /// costed population and no candidates, exactly as the corpus builder
    /// treats it. More than one episode is what makes the sharding tests
    /// mean anything: with a single episode every fan-out degenerates to
    /// one shard doing all the work.
    fn fixture_corpus_episodes_v1(
        scorer: &RunnerFixedScorerV1,
        episode_count: u64,
        take: usize,
    ) -> TtsS1CorpusManifestV1 {
        use crate::rl::TerminalClassificationV1;
        let base_seed = MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1];
        let mut episodes: Vec<TtsS1CorpusEpisodeV1> = Vec::new();
        let mut decisions: Vec<TtsS1CorpusDecisionV1> = Vec::new();
        let mut all_counts: Vec<u64> = Vec::new();
        let mut natural_counts: Vec<u64> = Vec::new();
        let mut truncated_episode_count = 0u64;
        let mut episode_id = 0u64;
        while (episodes.len() as u64) < episode_count {
            assert!(
                episode_id < episode_count + 8,
                "the fixture must find {episode_count} natural-terminal episodes"
            );
            let harvest = harvest_episode_v1(
                scorer,
                base_seed,
                episode_id,
                FIXTURE_MAX_PHYSICAL_DECISIONS_V1,
                FIXTURE_MAX_POLICY_STEPS_V1,
            )
            .expect("the fixture episode plays");
            let count = harvest.decisions.len() as u64;
            all_counts.push(count);
            if harvest.classification != TerminalClassificationV1::Natural {
                truncated_episode_count += 1;
                episode_id += 1;
                continue;
            }
            assert!(
                harvest.decisions.len() > take,
                "the fixture episode must be strictly longer than the target sample, so the \
                 whole-episode replay is provably more than the targets: got {} for {take}",
                harvest.decisions.len()
            );
            natural_counts.push(count);
            // Spread the targets across the episode rather than clustering
            // them at its start, so the surface checks land at genuinely
            // different accumulated histories.
            let stride = (harvest.decisions.len() / take).max(1);
            let actions = harvest.actions.clone();
            let environment_seed = harvest.decisions[0].coordinates.environment_seed;
            episodes.push(TtsS1CorpusEpisodeV1 {
                episode_id,
                episode_base_seed: base_seed,
                environment_seed,
                decision_count: count,
                terminal_classification:
                    crate::native_tts_s1_corpus_v1::terminal_classification_tag_v1(
                        TerminalClassificationV1::Natural,
                    )
                    .to_owned(),
                action_sequence: actions,
            });
            decisions.extend(
                harvest
                    .into_decisions_with_action_sequences_v1()
                    .into_iter()
                    .step_by(stride)
                    .take(take),
            );
            episode_id += 1;
        }
        let candidate_count = decisions.len() as u64;
        let natural_terminal_episode_count = natural_counts.len() as u64;
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        TtsS1CorpusManifestV1::seal_v1(corpus_body_v1(
            TtsS1CorpusCheckpointV1::from_identity_v1(&fixture_identity_v1(), &architecture),
            FIXTURE_SEED_BLOCK_ID_V1,
            base_seed,
            episode_id,
            FIXTURE_MAX_PHYSICAL_DECISIONS_V1,
            FIXTURE_MAX_POLICY_STEPS_V1,
            TtsS1CorpusSelectionV1 {
                decisions,
                episodes,
                candidate_count,
                natural_terminal_episode_count,
                truncated_episode_count,
                episode_decisions: TtsS1EpisodeDecisionStatsV1::summarize_v1(&natural_counts)
                    .expect("the natural episodes summarize"),
                all_episode_decisions: TtsS1AllEpisodeDecisionStatsV1::summarize_v1(
                    &all_counts,
                    natural_terminal_episode_count,
                    truncated_episode_count,
                )
                .expect("the harvested episodes summarize"),
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
            FIXTURE_MAX_EPISODES_V1,
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

    /// END TO END, t512, over a WHOLE episode: self-play produces a
    /// corpus, the corpus's episode reconstructs through the kernel, the
    /// production selector searches EVERY decision of it in order with the
    /// production diagnostics writer publishing behind it, and the report
    /// chains and verifies.
    ///
    /// IGNORED IN A DEBUG BUILD, and the reason is the measurement itself.
    /// Whole-episode replay searches every decision of the fixture episode
    /// (274 of them), and one t512 search costs seconds in an unoptimized
    /// build, so this test takes tens of minutes there and about half a
    /// minute in release. It cannot be made cheap by shortening the
    /// episode: see `FIXTURE_MAX_PHYSICAL_DECISIONS_V1` for why a short
    /// episode cannot be searched at all.
    ///
    ///     cargo test --release --lib native_tts_s1 -- --ignored
    ///
    /// Everything this test covers that does NOT cost a whole episode of
    /// searches is covered by the always-on tests below: the fail-closed
    /// reconstruction refusals, the episode guard, the terminal
    /// requirement, the corpus determinism, and all of the arithmetic.
    #[test]
    #[ignore = "whole-episode t512 replay: ~274 searches, minutes in a debug build; run with cargo test --release -- --ignored"]
    fn a_t512_replay_runs_end_to_end_over_a_freshly_built_corpus_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 3);
        let report = replay_fixture_v1(&scorer, &corpus, "e2e");
        let body = &report.body;

        assert_eq!(body.tier, "t512");
        assert_eq!(body.transition_budget, 512);
        // WHOLE EPISODES: every decision of the fixture episode is
        // searched, not only the three stratified targets, so each
        // published record carries the true accumulated history.
        assert_eq!(body.corpus_decision_count, 3);
        assert_eq!(body.corpus_targets_replayed, 3);
        assert_eq!(body.corpus_episode_count, 1);
        assert_eq!(body.episodes_replayed, 1);
        assert_eq!(
            body.searched_decisions, corpus.body.episodes[0].decision_count,
            "every decision of the episode must be searched"
        );
        assert!(
            body.searched_decisions > body.corpus_targets_replayed,
            "the whole episode must be strictly more than its targets"
        );
        assert_eq!(body.whole_episode_view.decisions, body.searched_decisions);
        assert_eq!(body.corpus_target_view.decisions, 3);
        assert_eq!(body.max_episodes, FIXTURE_MAX_EPISODES_V1);
        assert!(body.replayed_whole_corpus);
        assert!(!body.stability_halves_enabled);
        assert_eq!(body.corpus_sha256, corpus.corpus_sha256);
        assert_eq!(body.slo_micros, 4_000_000);
        assert_eq!(body.hard_timeout_micros, 20_000_000);
        assert_eq!(
            verify_tts_s1_replay_chain_v1(body).unwrap(),
            body.searched_decisions as usize
        );
        // The records accumulate in episode order, and exactly the target
        // ordinals are flagged.
        for (index, record) in body.decisions.iter().enumerate() {
            assert_eq!(record.record_ordinal, index as u64);
            assert_eq!(record.decision_ordinal, index as u64);
            assert_eq!(record.episode_id, 0);
        }
        let flagged: Vec<u64> = body
            .decisions
            .iter()
            .filter(|record| record.is_corpus_target)
            .map(|record| record.decision_ordinal)
            .collect();
        let expected_targets: Vec<u64> = corpus
            .body
            .decisions
            .iter()
            .map(|decision| decision.coordinates.decision_ordinal)
            .collect();
        assert_eq!(flagged, expected_targets);

        // The PROTOCOL latency is strictly more than the decision phase
        // alone, on every decision: the publication and the response tail
        // are real, measured, non-zero work, which is the whole point of
        // routing S1 through the production writer.
        assert!(
            body.whole_episode_view.protocol_wall_time.total_micros
                >= body.whole_episode_view.decision_wall_time.total_micros
        );
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
        assert_eq!(
            body.diagnostics_episode_files[0].decision_record_count,
            body.searched_decisions
        );
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
        // EVERY harvested episode is costed, and the population that was
        // costed is the corpus's own all-episode population.
        assert_eq!(
            cap.estimated_episode_count,
            corpus.body.all_episode_decisions.episode_count
        );
        assert_eq!(
            cap.natural_terminal_episode_count,
            corpus
                .body
                .all_episode_decisions
                .natural_terminal_episode_count
        );
        assert_eq!(
            cap.truncated_episode_count,
            corpus.body.all_episode_decisions.truncated_episode_count
        );
        assert_eq!(
            cap.episode_cost_estimates.len() as u64,
            cap.estimated_episode_count
        );
        for (estimate, decision_count) in cap
            .episode_cost_estimates
            .iter()
            .zip(corpus.body.all_episode_decisions.decision_counts.iter())
        {
            assert_eq!(estimate.decision_count, *decision_count);
            assert!(estimate.estimated_micros > 0);
        }
        // The curve was fitted to the WHOLE-EPISODE population, which is
        // the only one that can show per-ordinal growth, and it is
        // published so the estimate is recomputable from the artifact.
        assert_eq!(
            cap.latency_curve.observed_samples,
            body.whole_episode_view.decisions
        );
        assert_eq!(
            cap.latency_curve.last_observed_ordinal,
            body.searched_decisions - 1
        );
        assert!(cap.latency_curve.extrapolation_slope_micros_per_ordinal >= 1);
        assert!(!cap.latency_curve.knots.is_empty());
        // The fit really is monotone, on this real timing data.
        for ordinal in 1..body.searched_decisions {
            assert!(
                cap.latency_curve.latency_at_v1(ordinal)
                    >= cap.latency_curve.latency_at_v1(ordinal - 1)
            );
        }
        assert_eq!(
            cap.projected_elapsed_hours_at_workers_milli,
            cap.projected_worker_hours_milli / 16
        );
        assert_eq!(
            cap.projected_worker_hours_milli,
            project_s2_worker_hours_milli_v1(cap.mean_estimated_episode_micros)
        );
        assert_eq!(
            cap.within_cap,
            cap.projected_worker_hours_milli <= cap.cap_worker_hours_milli
        );

        // The LATENCY verdict is taken from the corpus targets, which is
        // the population the sketch defines S1 over, and the report says so.
        assert_eq!(body.verdict_view, TTS_S1_VERDICT_VIEW_V1);
        assert_eq!(body.verdict_view, "corpus_target_view");

        // EVERY searched decision, not only the targets: the corpus
        // targets are a subset of the records now, so these invariants are
        // asserted over the whole population the verdict is taken from.
        for record in &body.decisions {
            assert_eq!(record.episode_id, 0);
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

        // The TARGET records line up with the corpus targets, in order,
        // and each one really was searched at the state the corpus
        // recorded: the surface check that let it through compared the
        // legal-action count, so the record's count must match too.
        let target_records: Vec<&TtsS1ReplayDecisionRecordV1> = body
            .decisions
            .iter()
            .filter(|record| record.is_corpus_target)
            .collect();
        assert_eq!(target_records.len(), corpus.body.decisions.len());
        for (record, target) in target_records.iter().zip(corpus.body.decisions.iter()) {
            assert_eq!(record.episode_id, target.coordinates.episode_id);
            assert_eq!(record.decision_ordinal, target.coordinates.decision_ordinal);
            assert_eq!(record.legal_action_count, target.surface.legal_action_count);
            assert_eq!(record.acting_player, target.surface.acting_player);
        }

        // The report really is publishable and re-provable from bytes.
        let bytes = report.canonical_bytes_v1().unwrap();
        assert_eq!(decode_tts_s1_replay_report_v1(&bytes).unwrap(), report);
    }

    /// The chosen actions and the whole search product do not depend on
    /// wall time: two runs of the same tier over the same corpus strip
    /// equal. The S0 bit-identical-replay pattern, applied to S1.
    ///
    /// Ignored for the same reason as the end-to-end test above, doubled:
    /// it runs the whole episode twice.
    #[test]
    #[ignore = "two whole-episode t512 replays: ~548 searches, minutes in a debug build; run with cargo test --release -- --ignored"]
    fn the_chosen_action_is_independent_of_timing_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 2);
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
            parsed.body.whole_episode_view.decision_wall_time.max_micros
                >= parsed.body.whole_episode_view.search_wall_time.p50_micros
        );
    }

    /// The corpus manifest is reproducible byte for byte across two runs.
    #[test]
    fn the_corpus_manifest_is_byte_identical_across_two_runs_v1() {
        let first = fixture_corpus_v1(&RunnerFixedScorerV1::new_v1(), 4)
            .canonical_bytes_v1()
            .unwrap();
        let second = fixture_corpus_v1(&RunnerFixedScorerV1::new_v1(), 4)
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
        let corpus = fixture_corpus_v1(&scorer, 2);
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
                FIXTURE_MAX_EPISODES_V1,
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

        // The environment seed is checked at the RESET, against the
        // episode record, which is where it now lives: a tampered one must
        // be refused before a single decision is reconstructed, let alone
        // searched.
        let mut tampered = corpus.clone();
        tampered.body.episodes[0].environment_seed ^= 1;
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
            FIXTURE_MAX_EPISODES_V1,
            None,
            &directory,
        )
        .expect_err("a tampered environment seed must fail closed");
        assert!(
            matches!(
                error,
                TtsS1ReplayErrorV1::ReconstructionMismatch {
                    field: "environment_seed",
                    ..
                }
            ),
            "tampering the episode environment seed produced {error}"
        );
        // And `decode` refuses it too, because the episode's seed and its
        // targets' recorded seeds must agree.
        assert!(matches!(
            decode_tts_s1_corpus_v1(&tampered.canonical_bytes_v1().unwrap()),
            Err(TtsS1CorpusErrorV1::InvalidManifest)
        ));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The episode guard is fail-closed, and it fires BEFORE any search.
    #[test]
    fn a_corpus_above_the_episode_guard_is_refused_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 1);
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let identity = fixture_identity_v1();
        let directory = scratch_diagnostics_dir_v1("guard");
        let error = replay_corpus_body_v1(
            &scorer,
            &identity,
            &architecture,
            TtsS1CorpusCheckpointV1::from_identity_v1(&identity, &architecture),
            &corpus,
            KernelNativeSearchTierV1::T512,
            FIXTURE_SEED_BLOCK_ID_V1,
            MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1],
            // The corpus contributes one episode, so a guard of zero must
            // refuse it.
            0,
            None,
            &directory,
        )
        .expect_err("a corpus above the guard must be refused");
        assert!(matches!(
            error,
            TtsS1ReplayErrorV1::TooManyEpisodes {
                episodes: 1,
                max_episodes: 0
            }
        ));
        assert_eq!(error.code_v1(), "tts_s1_replay_too_many_episodes");
        // Nothing was published: the guard fires before the writer opens.
        assert!(!directory.join(TTS_S1_RESPONSE_LINES_FILE_V1).exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// An episode whose recorded sequence does not reach a terminal is
    /// refused: the episode the corpus describes is then not the episode
    /// the kernel just played, and no latency taken from it means anything.
    #[test]
    fn an_episode_that_does_not_reach_a_terminal_is_refused_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let mut corpus = fixture_corpus_v1(&scorer, 1);
        // Keep the first decision only. Its own recorded prefix is empty,
        // so the decode invariant still holds, but playing one action of a
        // 274-decision game does not end it.
        corpus.body.episodes[0].decision_count = 1;
        corpus.body.episodes[0].action_sequence.truncate(1);
        corpus.body.decisions.truncate(1);
        // The stated contributing population follows the episode list, or
        // `decode` refuses the fixture before the replay ever sees it.
        corpus.body.contributing_episode_decisions = 1;
        let corpus = TtsS1CorpusManifestV1::seal_v1(corpus.body).unwrap();
        assert!(
            decode_tts_s1_corpus_v1(&corpus.canonical_bytes_v1().unwrap()).is_ok(),
            "the truncated fixture must still be a well-formed corpus"
        );

        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let identity = fixture_identity_v1();
        let directory = scratch_diagnostics_dir_v1("unterminated");
        let error = replay_corpus_body_v1(
            &scorer,
            &identity,
            &architecture,
            TtsS1CorpusCheckpointV1::from_identity_v1(&identity, &architecture),
            &corpus,
            KernelNativeSearchTierV1::T512,
            FIXTURE_SEED_BLOCK_ID_V1,
            MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1],
            FIXTURE_MAX_EPISODES_V1,
            None,
            &directory,
        )
        .expect_err("an unterminated episode must be refused");
        assert!(matches!(
            error,
            TtsS1ReplayErrorV1::EpisodeDidNotTerminate { episode_id: 0 }
        ));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The replay refuses a corpus drawn from a different checkpoint.
    #[test]
    fn a_foreign_corpus_checkpoint_fails_closed_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 1);
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
                FIXTURE_MAX_EPISODES_V1,
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

    /// A synthetic per-decision sample set with the growth the production
    /// writer actually exhibits: publication cost rises with the decision
    /// ordinal, because the whole episode file is republished each time.
    fn synthetic_curve_samples_v1() -> Vec<(u64, u64)> {
        (0..100u64)
            .map(|ordinal| (ordinal, 1_000 + 10 * ordinal))
            .collect()
    }

    #[test]
    fn the_latency_curve_is_monotone_and_never_flat_past_its_evidence_v1() {
        let curve = TtsS1LatencyCurveV1::fit_v1(&synthetic_curve_samples_v1()).unwrap();
        assert_eq!(curve.rule, TTS_S1_LATENCY_CURVE_RULE_V2);
        assert_eq!(curve.observed_samples, 100);
        assert_eq!(curve.last_observed_ordinal, 99);
        // The input is already monotone, so the isotonic fit reproduces it
        // exactly: one knot per ordinal, at that ordinal's own value.
        assert_eq!(curve.knots.len(), 100);
        for ordinal in 0..100u64 {
            assert_eq!(curve.latency_at_v1(ordinal), 1_000 + 10 * ordinal);
        }
        // Past the evidence it continues at the steepest fitted slope, and
        // never flat.
        assert_eq!(curve.extrapolation_slope_micros_per_ordinal, 10);
        assert_eq!(curve.latency_at_v1(100), 1_990 + 10);
        assert_eq!(curve.latency_at_v1(200), 1_990 + 10 * 101);
        // Monotone everywhere, observed and extrapolated alike.
        for ordinal in 1..300u64 {
            assert!(curve.latency_at_v1(ordinal) >= curve.latency_at_v1(ordinal - 1));
        }
    }

    #[test]
    fn the_latency_curve_pools_adjacent_violators_v1() {
        // A dip in the middle must be pooled away, not preserved: the fit
        // is monotone non-decreasing by construction.
        let samples = vec![(0u64, 100u64), (1, 900), (2, 300), (3, 1_000)];
        let curve = TtsS1LatencyCurveV1::fit_v1(&samples).unwrap();
        for ordinal in 1..4u64 {
            assert!(
                curve.latency_at_v1(ordinal) >= curve.latency_at_v1(ordinal - 1),
                "the fit must be non-decreasing at {ordinal}"
            );
        }
        // Ordinals 1 and 2 pool to their mean, 600.
        assert_eq!(curve.latency_at_v1(1), 600);
        assert_eq!(curve.latency_at_v1(2), 600);
        assert_eq!(curve.latency_at_v1(0), 100);
        assert_eq!(curve.latency_at_v1(3), 1_000);
        // A wholly flat sample set still extrapolates upward, by the one
        // micro per ordinal floor.
        let flat = TtsS1LatencyCurveV1::fit_v1(&[(0, 500), (1, 500), (2, 500)]).unwrap();
        assert_eq!(flat.extrapolation_slope_micros_per_ordinal, 1);
        assert!(flat.latency_at_v1(3) > flat.latency_at_v1(2));
        assert!(TtsS1LatencyCurveV1::fit_v1(&[]).is_none());
    }

    /// REGRESSION. Repeated ordinals must be aggregated BEFORE
    /// pool-adjacent-violators runs.
    ///
    /// An earlier fit folded a second sample at an already-seen ordinal
    /// into the running block and then skipped the violation check, so a
    /// lower later sample at that ordinal dragged the block below its
    /// predecessor and the output was not monotone at all: this exact input
    /// fitted to 100, 50, 100. Every replayed episode contributes a sample
    /// at every ordinal it reaches, so repeated ordinals are the normal
    /// case here, not an edge one.
    #[test]
    fn repeated_ordinals_are_aggregated_before_pooling_v1() {
        let samples = vec![(0u64, 100u64), (1, 100), (1, 0), (2, 100)];
        let curve = TtsS1LatencyCurveV1::fit_v1(&samples).unwrap();
        for ordinal in 1..3u64 {
            assert!(
                curve.latency_at_v1(ordinal) >= curve.latency_at_v1(ordinal - 1),
                "the fit must be non-decreasing at {ordinal}, got {:?}",
                (0..3).map(|o| curve.latency_at_v1(o)).collect::<Vec<_>>()
            );
        }
        // Ordinal 1's two samples pool to a mean of 50, which violates
        // ordinal 0's 100, so 0 and 1 merge to (100 + 100 + 0) / 3 = 66.
        assert_eq!(curve.latency_at_v1(0), 66);
        assert_eq!(curve.latency_at_v1(1), 66);
        assert_eq!(curve.latency_at_v1(2), 100);
        assert_eq!(curve.observed_samples, 4);
        assert_eq!(curve.last_observed_ordinal, 2);
        // The sample counts are preserved by the aggregation, not lost.
        assert_eq!(
            curve
                .knots
                .iter()
                .map(|knot| knot.sample_count)
                .sum::<u64>(),
            4
        );
    }

    /// REGRESSION. The extrapolation slope is the largest rise between two
    /// ADJACENT fitted ordinals, not a block-value jump divided by the
    /// distance between block ends.
    ///
    /// These samples fit to blocks [0] = 100, [1..2] = 600, [3] = 1000. The
    /// curve's steepest single-ordinal step is 500, from ordinal 0 to
    /// ordinal 1. An earlier form divided that 500 by the two-ordinal
    /// distance between the blocks' ends and reported 400, understating the
    /// step the curve actually takes and therefore under-costing every
    /// extrapolated ordinal.
    #[test]
    fn the_extrapolation_slope_is_the_largest_adjacent_rise_v1() {
        let curve =
            TtsS1LatencyCurveV1::fit_v1(&[(0u64, 100u64), (1, 900), (2, 300), (3, 1_000)]).unwrap();
        assert_eq!(curve.latency_at_v1(0), 100);
        assert_eq!(curve.latency_at_v1(1), 600);
        assert_eq!(curve.latency_at_v1(2), 600);
        assert_eq!(curve.latency_at_v1(3), 1_000);
        // max(600 - 100, 1000 - 600) = 500, and NOT 500 / 2 = 250 nor the
        // 400 the divided form reported.
        assert_eq!(curve.extrapolation_slope_micros_per_ordinal, 500);
        assert_eq!(curve.latency_at_v1(4), 1_500);
        assert_eq!(curve.latency_at_v1(5), 2_000);
    }

    /// THE CONSERVATISM CLAIM. A truncated episode longer than anything
    /// replayed must be costed above every replayed episode, because
    /// publication cost grows with the ordinal and a flat mean would charge
    /// its long tail at a short episode's average.
    #[test]
    fn an_episode_longer_than_any_replayed_one_is_costed_above_them_all_v1() {
        let curve = TtsS1LatencyCurveV1::fit_v1(&synthetic_curve_samples_v1()).unwrap();
        // The replayed episodes are all at most as long as the evidence.
        let replayed = [40u64, 75, 100];
        let longest_replayed = replayed
            .iter()
            .map(|count| curve.episode_cost_micros_v1(*count).0)
            .max()
            .unwrap();
        // A truncated episode that ran into the decision cap, far beyond
        // anything observed.
        let (truncated_cost, extrapolated) = curve.episode_cost_micros_v1(1_024);
        assert!(
            truncated_cost > longest_replayed,
            "the truncated episode must cost more than every replayed one"
        );
        assert_eq!(extrapolated, 1_024 - 100);
        // And strictly more than a flat-mean estimate would have charged:
        // the mean observed latency times its length.
        let flat_mean = u128::from(
            synthetic_curve_samples_v1()
                .iter()
                .map(|(_, micros)| micros)
                .sum::<u64>()
                / 100,
        );
        assert!(
            truncated_cost > flat_mean * 1_024,
            "the curve must charge the long tail above a flat mean"
        );
        // Cost is monotone in episode length.
        assert!(curve.episode_cost_micros_v1(200).0 > curve.episode_cost_micros_v1(199).0);
    }

    #[test]
    fn the_compute_cap_projection_costs_every_harvested_episode_v1() {
        // A flat 1 s curve over 300 ordinals, so the arithmetic is
        // checkable by hand: each 300-decision episode costs 300 s, and
        // 6,144 games x 300 s = 1,843,200 s = 512 worker-hours. No
        // division by the worker count: worker-hours are work.
        let samples: Vec<(u64, u64)> = (0..300u64).map(|ordinal| (ordinal, 1_000_000)).collect();
        let population = TtsS1AllEpisodeDecisionStatsV1::summarize_v1(&[300], 1, 0).unwrap();
        let projection = compute_cap_projection_v1(&samples, &population).unwrap();
        assert_eq!(projection.mean_estimated_episode_micros, 300_000_000);
        assert_eq!(projection.projected_worker_hours_milli, 512_000);
        assert_eq!(projection.projected_elapsed_hours_at_workers_milli, 32_000);
        assert!(!projection.within_cap);
        assert_eq!(projection.s2_wrapped_games, 6_144);
        assert_eq!(projection.s2_workers, 16);
        assert_eq!(projection.cap_worker_hours_milli, 48_000);
        assert!(projection.raw_policy_games_excluded);
        assert_eq!(projection.extrapolated_ordinals, 0);

        // EVERY harvested episode is costed individually, natural and
        // truncated alike, and the truncated one is charged for the
        // ordinals nobody replayed.
        let population =
            TtsS1AllEpisodeDecisionStatsV1::summarize_v1(&[100, 300, 1_024], 2, 1).unwrap();
        let projection = compute_cap_projection_v1(&samples, &population).unwrap();
        assert_eq!(projection.estimated_episode_count, 3);
        assert_eq!(projection.natural_terminal_episode_count, 2);
        assert_eq!(projection.truncated_episode_count, 1);
        assert_eq!(projection.episode_cost_estimates.len(), 3);
        assert_eq!(projection.episode_cost_estimates[2].decision_count, 1_024);
        assert_eq!(
            projection.episode_cost_estimates[2].extrapolated_ordinals,
            1_024 - 300
        );
        assert_eq!(projection.extrapolated_ordinals, 1_024 - 300);
        // The longest episode is the most expensive, and it is the one that
        // sets the maximum.
        assert_eq!(
            projection.max_estimated_episode_micros,
            projection.episode_cost_estimates[2].estimated_micros
        );
        // The curve and its knots are published, so the estimate is
        // recomputable from the artifact alone.
        assert!(!projection.latency_curve.knots.is_empty());
        assert_eq!(projection.latency_curve.rule, TTS_S1_LATENCY_CURVE_RULE_V2);
        assert!(
            projection
                .latency_curve
                .extrapolation_slope_micros_per_ordinal
                >= 1
        );
        assert!(compute_cap_projection_v1(&[], &population).is_none());
    }

    fn synthetic_record_v1(ordinal: u64, previous: String) -> TtsS1ReplayDecisionRecordV1 {
        TtsS1ReplayDecisionRecordV1 {
            record_ordinal: ordinal,
            previous_record_sha256: previous,
            episode_id: 0,
            decision_ordinal: ordinal,
            is_corpus_target: true,
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
        let borrowed: Vec<&TtsS1ReplayDecisionRecordV1> = records.iter().collect();
        let view = latency_view_v1(&borrowed).unwrap();
        let _ = (&search, &decision);
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
            corpus_episode_count: 1,
            episodes_replayed: 1,
            searched_decisions: record_count,
            corpus_targets_replayed: record_count,
            max_episodes: 8,
            replayed_whole_corpus: true,
            percentile_rule: TTS_S1_PERCENTILE_RULE_V1.to_owned(),
            verdict_view: TTS_S1_VERDICT_VIEW_V1.to_owned(),
            corpus_target_view: view.clone(),
            whole_episode_view: view,
            slo_micros: slo_micros_v1(),
            hard_timeout_micros: hard_timeout_micros_v1(),
            shard_topology: TtsS1ShardTopologyV1::evaluate_v1(
                TTS_S1_FORMAL_SHARD_COUNT_V1,
                TtsS1HostFactsV1 {
                    logical_cpus: 32,
                    total_memory_bytes: 137_438_953_472,
                },
            ),
            diagnostics_episode_files: vec![TtsS1DiagnosticsEpisodeFileV1 {
                episode_id: 0,
                file_name: "episode-synthetic.jsonl".to_owned(),
                bytes: 4_096,
                sha256: "cd".repeat(32),
                decision_record_count: record_count,
            }],
            compute_cap: compute_cap_projection_v1(
                &synthetic_curve_samples_v1(),
                &TtsS1AllEpisodeDecisionStatsV1::summarize_v1(&[300], 1, 0).unwrap(),
            )
            .unwrap(),
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
        let borrowed: Vec<&TtsS1ReplayDecisionRecordV1> = slow.decisions.iter().collect();
        let view = latency_view_v1(&borrowed).unwrap();
        let searched = slow.decisions.len() as u64;
        let _ = searched;
        slow.compute_cap = compute_cap_projection_v1(
            &synthetic_curve_samples_v1(),
            &TtsS1AllEpisodeDecisionStatsV1::summarize_v1(&[300], 1, 0).unwrap(),
        )
        .unwrap();
        let (verdict, reason) = verdict_v1(
            view.p99_protocol_ceiling_status,
            view.max_protocol_ceiling_status,
            slow.compute_cap.within_cap,
            true,
        );
        slow.corpus_target_view = view.clone();
        slow.whole_episode_view = view;
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

    // -----------------------------------------------------------------
    // SHARDING. The split, the shard report, and the merge.
    // -----------------------------------------------------------------

    #[test]
    fn the_shard_selector_partitions_every_position_exactly_once_v1() {
        for count in [1u64, 2, 3, 8, 64] {
            for position in 0..200u64 {
                let owners = (0..count)
                    .filter(|index| {
                        TtsS1ShardSelectorV1::new_v1(*index, count)
                            .expect("a legal shard")
                            .owns_position_v1(position)
                    })
                    .count();
                assert_eq!(
                    owners, 1,
                    "position {position} must be owned by exactly one of {count} shards"
                );
            }
        }
        // The constructor is the whole range check: an index at or above
        // the count, a zero count, and a count past the bound are all
        // unrepresentable rather than merely rejected later.
        assert!(TtsS1ShardSelectorV1::new_v1(0, 0).is_none());
        assert!(TtsS1ShardSelectorV1::new_v1(8, 8).is_none());
        assert!(TtsS1ShardSelectorV1::new_v1(0, TTS_S1_MAX_SHARD_COUNT_V1 + 1).is_none());
        assert!(TtsS1ShardSelectorV1::new_v1(63, TTS_S1_MAX_SHARD_COUNT_V1).is_some());
    }

    /// A synthetic whole-run record set over `decision_counts` episodes,
    /// carrying the per-ordinal publication growth the production writer
    /// exhibits, so the isotonic fit has real shape to reproduce.
    fn synthetic_run_records_v1(decision_counts: &[u64]) -> Vec<TtsS1ReplayDecisionRecordV1> {
        let mut records = Vec::new();
        for (position, count) in decision_counts.iter().enumerate() {
            for ordinal in 0..*count {
                let mut record = synthetic_record_v1(ordinal, String::new());
                // Episode ids are deliberately NOT the positions: the
                // merge reassembles by position, and an implementation
                // that quietly sorted by episode id instead would pass a
                // fixture in which the two agreed.
                record.episode_id = 100 - position as u64;
                record.decision_ordinal = ordinal;
                record.is_corpus_target = ordinal % 3 == 0;
                record.wall_time = TtsS1DecisionWallTimeV1 {
                    search_micros: 1_000 + ordinal,
                    decision_micros: 2_000 + ordinal,
                    publish_micros: 300 + 7 * ordinal,
                    response_micros: 100 + ordinal,
                    protocol_micros: 2_400 + 9 * ordinal,
                };
                records.push(record);
            }
        }
        records
    }

    fn synthetic_diagnostics_files_v1(
        decision_counts: &[u64],
    ) -> Vec<TtsS1DiagnosticsEpisodeFileV1> {
        decision_counts
            .iter()
            .enumerate()
            .map(|(position, count)| TtsS1DiagnosticsEpisodeFileV1 {
                episode_id: 100 - position as u64,
                file_name: format!("episode-{position}.jsonl"),
                bytes: 4_096 + position as u64,
                sha256: format!("{position:02x}").repeat(32),
                decision_record_count: *count,
            })
            .collect()
    }

    fn synthetic_identity_v1(
        decision_counts: &[u64],
        corpus_decision_count: u64,
    ) -> TtsS1ReplayIdentityV1 {
        let template = synthetic_body_v1(1);
        TtsS1ReplayIdentityV1 {
            // The merge refuses a shard produced by another build, so the
            // fixture states THIS build's commit rather than a literal.
            engine_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_owned(),
            tier: template.tier,
            transition_budget: template.transition_budget,
            policy_step_depth_cap: template.policy_step_depth_cap,
            seed_block_id: template.seed_block_id,
            seed_block_seed: template.seed_block_seed,
            stability_halves_enabled: false,
            checkpoint: template.checkpoint,
            wrapper_identity: template.wrapper_identity,
            search_authority_digest_sha256: template.search_authority_digest_sha256,
            corpus_sha256: template.corpus_sha256,
            corpus_decision_count,
            corpus_episode_count: decision_counts.len() as u64,
            episodes_replayed: decision_counts.len() as u64,
            max_episodes: 64,
            percentile_rule: TTS_S1_PERCENTILE_RULE_V1.to_owned(),
            verdict_view: TTS_S1_VERDICT_VIEW_V1.to_owned(),
            slo_micros: slo_micros_v1(),
            hard_timeout_micros: hard_timeout_micros_v1(),
            chain_genesis_sha256: TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned(),
            // A host large enough for the pinned formal topology, so the
            // fixture can exercise both the formal and the non-formal
            // outcome from the shard count alone.
            host_logical_cpus: 32,
            host_total_memory_bytes: 137_438_953_472,
            all_episode_decisions: TtsS1AllEpisodeDecisionStatsV1::summarize_v1(
                decision_counts,
                decision_counts.len() as u64,
                0,
            )
            .expect("the synthetic population summarizes"),
        }
    }

    /// Splits one synthetic run into `shard_count` shard bodies, exactly as
    /// K replay processes would have produced them.
    fn synthetic_shard_bodies_v1(
        decision_counts: &[u64],
        shard_count: u64,
    ) -> Vec<TtsS1ReplayShardReportBodyV1> {
        let records = synthetic_run_records_v1(decision_counts);
        let files = synthetic_diagnostics_files_v1(decision_counts);
        let targets = records
            .iter()
            .filter(|record| record.is_corpus_target)
            .count() as u64;
        let identity = synthetic_identity_v1(decision_counts, targets);
        // Where each episode's records start in the whole run.
        let mut offsets: Vec<usize> = Vec::with_capacity(decision_counts.len());
        let mut running = 0usize;
        for count in decision_counts {
            offsets.push(running);
            running += *count as usize;
        }

        let mut bodies = Vec::with_capacity(shard_count as usize);
        for shard_index in 0..shard_count {
            let selector =
                TtsS1ShardSelectorV1::new_v1(shard_index, shard_count).expect("a legal shard");
            let mut shard_records: Vec<TtsS1ReplayDecisionRecordV1> = Vec::new();
            let mut shard_episodes: Vec<TtsS1ShardEpisodeV1> = Vec::new();
            let mut shard_files: Vec<TtsS1DiagnosticsEpisodeFileV1> = Vec::new();
            let mut shard_targets = 0u64;
            for (position, count) in decision_counts.iter().enumerate() {
                if !selector.owns_position_v1(position as u64) {
                    continue;
                }
                let first = offsets[position];
                let slice = &records[first..first + *count as usize];
                let episode_targets = slice
                    .iter()
                    .filter(|record| record.is_corpus_target)
                    .count() as u64;
                shard_episodes.push(TtsS1ShardEpisodeV1 {
                    episode_position: position as u64,
                    episode_id: slice[0].episode_id,
                    episode_base_seed: identity.seed_block_seed,
                    decision_count: *count,
                    searched_decisions: *count,
                    corpus_targets_replayed: episode_targets,
                    first_record_index: shard_records.len() as u64,
                    elapsed_micros: 1_234 + position as u64,
                    protocol_micros_total: slice.iter().fold(0u64, |running, record| {
                        running + record.wall_time.protocol_micros
                    }),
                });
                shard_files.push(files[position].clone());
                shard_targets += episode_targets;
                shard_records.extend(slice.iter().cloned());
            }
            let mut previous = TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned();
            for (ordinal, record) in shard_records.iter_mut().enumerate() {
                record.record_ordinal = ordinal as u64;
                record.previous_record_sha256 = previous.clone();
                previous = lower_hex_sha256_v4(record.chain_link_v1().unwrap());
            }
            bodies.push(TtsS1ReplayShardReportBodyV1 {
                identity: identity.clone(),
                shard_assignment_rule: TTS_S1_SHARD_ASSIGNMENT_RULE_V1.to_owned(),
                shard_index,
                shard_count,
                shard_episodes_replayed: shard_episodes.len() as u64,
                searched_decisions: shard_records.len() as u64,
                corpus_targets_replayed: shard_targets,
                episodes: shard_episodes,
                diagnostics_episode_files: shard_files,
                chain_genesis_sha256: TTS_S1_REPLAY_CHAIN_GENESIS_V1.to_owned(),
                final_record_sha256: previous,
                decisions: shard_records,
            });
        }
        bodies
    }

    fn write_shard_reports_v1(directory: &Path, bodies: &[TtsS1ReplayShardReportBodyV1]) {
        for body in bodies {
            let report =
                TtsS1ReplayShardReportV1::seal_v1(body.clone()).expect("the shard report seals");
            let path = directory.join(tts_s1_shard_report_file_name_v1(
                body.shard_index,
                body.shard_count,
            ));
            publish_tts_s1_replay_shard_report_v1(&report, &path)
                .expect("the shard report publishes");
        }
    }

    /// What ONE process finalizing these records under `shard_count`
    /// concurrency publishes. The merge of that many shards has to
    /// reproduce it byte for byte.
    fn synthetic_finalized_bytes_v1(decision_counts: &[u64], shard_count: u64) -> Vec<u8> {
        let records = synthetic_run_records_v1(decision_counts);
        let targets = records
            .iter()
            .filter(|record| record.is_corpus_target)
            .count() as u64;
        let identity = synthetic_identity_v1(decision_counts, targets);
        let body = finalize_tts_s1_replay_body_v1(
            &identity,
            records,
            synthetic_diagnostics_files_v1(decision_counts),
            targets,
            shard_count,
        )
        .expect("the synthetic body finalizes");
        TtsS1ReplayReportV1::seal_v1(body)
            .expect("the synthetic report seals")
            .canonical_bytes_v1()
            .expect("the synthetic report encodes")
    }

    /// THE CLAIM SHARDING HAS TO KEEP, stated over timings that are held
    /// fixed so it can be stated as BYTE equality rather than as equality
    /// after a stripping.
    ///
    /// The same records, the same episodes, the same diagnostics files: the
    /// merge of K shard reports is byte for byte the report one process
    /// finalizing those records would have published, for every K from one
    /// to the episode count. That covers the chain (re-assigned over the
    /// union in position order), both views, the pooled isotonic fit, every
    /// per-episode cost estimate, the projection and the verdict at once,
    /// because all of them are inside those bytes.
    ///
    /// The ONE thing that legitimately differs between a one-process run
    /// and a K-process one is the recorded topology, which says how many
    /// processes were contending for the machine while these wall times
    /// were taken. That is asserted separately, and asserted to be the only
    /// difference.
    #[test]
    fn a_merged_report_is_byte_identical_to_the_unsharded_one_v1() {
        let counts = [7u64, 5, 9, 4, 6];
        let unsharded = synthetic_finalized_bytes_v1(&counts, 1);
        // The fixture is not degenerate: the report really carries the
        // records, the curve and a verdict.
        let decoded = decode_tts_s1_replay_report_v1(&unsharded).expect("the fixture re-proves");
        assert_eq!(decoded.body.searched_decisions, counts.iter().sum::<u64>());
        assert!(decoded.body.compute_cap.latency_curve.knots.len() > 1);
        let unsharded_stripped = strip_timing_fields_v1(&unsharded).unwrap();

        for shard_count in [1u64, 2, 3, 5] {
            let directory = scratch_diagnostics_dir_v1(&format!("merge-{shard_count}"));
            let bodies = synthetic_shard_bodies_v1(&counts, shard_count);
            // Every shard is a self-verifying document before anything is
            // merged, and the shards partition the run.
            assert_eq!(bodies.len() as u64, shard_count);
            assert_eq!(
                bodies
                    .iter()
                    .map(|body| body.searched_decisions)
                    .sum::<u64>(),
                counts.iter().sum::<u64>()
            );
            write_shard_reports_v1(&directory, &bodies);
            let merged =
                merge_tts_s1_replay_shards_v1(&directory, shard_count).expect("the shards merge");
            let merged_bytes = merged
                .canonical_bytes_v1()
                .expect("the merged report encodes");
            assert_eq!(
                merged_bytes,
                synthetic_finalized_bytes_v1(&counts, shard_count),
                "the merge of {shard_count} shards must be what one process finalizing the same \
                 records at the same concurrency publishes, byte for byte"
            );
            // The topology is the ONLY thing a different fan-out changes:
            // strip the fields a re-run may change and the merged report is
            // the one-process report exactly.
            assert_eq!(merged.body.shard_topology.shard_count, shard_count);
            assert_eq!(
                merged.body.shard_topology.formal_shard_count,
                TTS_S1_FORMAL_SHARD_COUNT_V1
            );
            assert_eq!(
                strip_timing_fields_v1(&merged_bytes).unwrap(),
                unsharded_stripped,
                "past the topology and the timings, {shard_count} shards and one process must \
                 publish the same report"
            );
            let _ = std::fs::remove_dir_all(&directory);
        }
    }

    /// THE PINNED FORMAL TOPOLOGY. Every wall time in the report is a
    /// sample from a loaded machine, so the concurrency is part of what the
    /// numbers mean; the report says which one it ran at, whether that is
    /// the pinned one, and why not when it is not.
    #[test]
    fn only_the_pinned_topology_on_a_large_enough_host_is_formal_v1() {
        // Eight shards on a host with two logical CPUs per shard: formal.
        let formal = TtsS1ShardTopologyV1::evaluate_v1(
            TTS_S1_FORMAL_SHARD_COUNT_V1,
            TtsS1HostFactsV1 {
                logical_cpus: 16,
                total_memory_bytes: 68_719_476_736,
            },
        );
        assert!(formal.meets_formal_topology);
        assert_eq!(formal.rule, TTS_S1_SHARD_TOPOLOGY_RULE_V1);
        assert_eq!(formal.shard_count, 8);
        assert_eq!(formal.formal_shard_count, 8);
        assert_eq!(formal.formal_logical_cpus_per_shard, 2);
        assert_eq!(formal.host_logical_cpus, 16);
        assert_eq!(formal.host_total_memory_bytes, 68_719_476_736);
        assert!(formal.formal_topology_reason.contains("16 logical CPUs"));

        // ANY other count, above or below, and the unsharded run itself.
        for shard_count in [1u64, 2, 4, 7, 9, 16] {
            let topology = TtsS1ShardTopologyV1::evaluate_v1(
                shard_count,
                TtsS1HostFactsV1 {
                    logical_cpus: 128,
                    total_memory_bytes: 1,
                },
            );
            assert!(
                !topology.meets_formal_topology,
                "{shard_count} concurrent processes is not the pinned topology"
            );
            assert!(topology.formal_topology_reason.contains("not the pinned 8"));
        }

        // The pinned count on a host too small for it: not formal, and the
        // reason names the host rather than the count.
        let cramped = TtsS1ShardTopologyV1::evaluate_v1(
            TTS_S1_FORMAL_SHARD_COUNT_V1,
            TtsS1HostFactsV1 {
                logical_cpus: 15,
                total_memory_bytes: 0,
            },
        );
        assert!(!cramped.meets_formal_topology);
        assert!(cramped.formal_topology_reason.contains("15 logical CPUs"));
        assert!(cramped.formal_topology_reason.contains("below the 16"));
        assert!(!cramped.formal_topology_reason.contains("not the pinned 8"));
        // Exactly at the boundary is admissible; one below is not.
        assert!(
            TtsS1ShardTopologyV1::evaluate_v1(
                TTS_S1_FORMAL_SHARD_COUNT_V1,
                TtsS1HostFactsV1 {
                    logical_cpus: 16,
                    total_memory_bytes: 0,
                },
            )
            .meets_formal_topology
        );

        // BOTH clauses are named when both fail, as the verdict does.
        let neither = TtsS1ShardTopologyV1::evaluate_v1(
            4,
            TtsS1HostFactsV1 {
                logical_cpus: 2,
                total_memory_bytes: 0,
            },
        );
        assert!(neither.formal_topology_reason.contains("not the pinned 8"));
        assert!(neither.formal_topology_reason.contains("2 logical CPUs"));
    }

    /// The host facts are really read, not defaulted: a process running
    /// this test has at least one logical CPU.
    #[test]
    fn the_host_facts_are_read_from_the_host_v1() {
        let host = TtsS1HostFactsV1::read_v1();
        assert!(
            host.logical_cpus >= 1,
            "a running process has at least one logical CPU"
        );
        // Total memory is 0 only where the platform declined to answer;
        // on every platform this crate builds for it does answer.
        #[cfg(any(windows, unix))]
        assert!(
            host.total_memory_bytes > 0,
            "the host must report its physical memory"
        );
        // Read twice, same answer: nothing here samples a moving quantity.
        assert_eq!(host, TtsS1HostFactsV1::read_v1());
    }

    /// Every way the K reports could fail to be one run is refused. None of
    /// them is a warning and none is a dropped shard: a partial union would
    /// publish a real-looking verdict over a population nobody measured.
    #[test]
    fn the_merge_fails_closed_on_anything_but_one_whole_run_v1() {
        let counts = [7u64, 5, 9, 4];
        let shard_count = 3u64;

        // 1. A MISSING SHARD.
        let directory = scratch_diagnostics_dir_v1("merge-missing");
        let bodies = synthetic_shard_bodies_v1(&counts, shard_count);
        write_shard_reports_v1(&directory, &bodies[..2]);
        assert!(matches!(
            merge_tts_s1_replay_shards_v1(&directory, shard_count),
            Err(TtsS1ReplayErrorV1::ShardMerge(_))
        ));
        // 2. A STRAY report from a different fan-out sitting beside them.
        write_shard_reports_v1(&directory, &bodies[2..]);
        assert!(merge_tts_s1_replay_shards_v1(&directory, shard_count).is_ok());
        write_shard_reports_v1(&directory, &synthetic_shard_bodies_v1(&counts, 1));
        assert!(matches!(
            merge_tts_s1_replay_shards_v1(&directory, shard_count),
            Err(TtsS1ReplayErrorV1::ShardMerge(_))
        ));
        let _ = std::fs::remove_dir_all(&directory);

        // 3. A SHARD COUNT the reports do not agree with.
        let directory = scratch_diagnostics_dir_v1("merge-count");
        write_shard_reports_v1(&directory, &synthetic_shard_bodies_v1(&counts, shard_count));
        assert!(matches!(
            merge_tts_s1_replay_shards_v1(&directory, 2),
            Err(TtsS1ReplayErrorV1::ShardMerge(_))
        ));
        for bad in [0u64, TTS_S1_MAX_SHARD_COUNT_V1 + 1] {
            assert!(matches!(
                merge_tts_s1_replay_shards_v1(&directory, bad),
                Err(TtsS1ReplayErrorV1::ShardMerge(_))
            ));
        }
        let _ = std::fs::remove_dir_all(&directory);

        // 4. A SHARD FROM ANOTHER RUN: same shape, different corpus.
        let directory = scratch_diagnostics_dir_v1("merge-foreign");
        let mut bodies = synthetic_shard_bodies_v1(&counts, shard_count);
        bodies[1].identity.corpus_sha256 = "ee".repeat(32);
        write_shard_reports_v1(&directory, &bodies);
        assert!(matches!(
            merge_tts_s1_replay_shards_v1(&directory, shard_count),
            Err(TtsS1ReplayErrorV1::ShardMerge(_))
        ));
        let _ = std::fs::remove_dir_all(&directory);

        // 5. A SHARD FROM ANOTHER BUILD.
        let directory = scratch_diagnostics_dir_v1("merge-build");
        let mut bodies = synthetic_shard_bodies_v1(&counts, shard_count);
        for body in bodies.iter_mut() {
            body.identity.engine_commit = "deadbeef".to_owned();
        }
        write_shard_reports_v1(&directory, &bodies);
        assert!(matches!(
            merge_tts_s1_replay_shards_v1(&directory, shard_count),
            Err(TtsS1ReplayErrorV1::ShardMerge(_))
        ));
        let _ = std::fs::remove_dir_all(&directory);

        // 6. A SHARD CLAIMING AN EPISODE IT DOES NOT OWN, which is how a
        //    duplicated episode would reach the union.
        let directory = scratch_diagnostics_dir_v1("merge-duplicate");
        let mut bodies = synthetic_shard_bodies_v1(&counts, shard_count);
        bodies[1].episodes[0].episode_position = 0;
        write_shard_reports_v1(&directory, &bodies);
        assert!(matches!(
            merge_tts_s1_replay_shards_v1(&directory, shard_count),
            Err(TtsS1ReplayErrorV1::ShardMerge(_))
        ));
        let _ = std::fs::remove_dir_all(&directory);

        // 7. A TAMPERED RECORD, which breaks the shard's own chain and is
        //    caught before the union is even assembled.
        let directory = scratch_diagnostics_dir_v1("merge-tampered");
        let mut bodies = synthetic_shard_bodies_v1(&counts, shard_count);
        bodies[0].decisions[0].chosen_action_index += 1;
        write_shard_reports_v1(&directory, &bodies);
        assert!(matches!(
            merge_tts_s1_replay_shards_v1(&directory, shard_count),
            Err(TtsS1ReplayErrorV1::BrokenChain)
        ));
        let _ = std::fs::remove_dir_all(&directory);

        // 8. A SHARD WHOSE EPISODE LIST DOES NOT PARTITION ITS RECORDS.
        let directory = scratch_diagnostics_dir_v1("merge-partition");
        let mut bodies = synthetic_shard_bodies_v1(&counts, shard_count);
        bodies[0].episodes[0].searched_decisions += 1;
        write_shard_reports_v1(&directory, &bodies);
        assert!(matches!(
            merge_tts_s1_replay_shards_v1(&directory, shard_count),
            Err(TtsS1ReplayErrorV1::ShardMerge(_))
        ));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A shard report re-proves from its own bytes, and is not a tier
    /// report: the two schemas are mutually undecodable, so a shard can
    /// never be read as a finished tier or the reverse.
    #[test]
    fn a_shard_report_round_trips_and_is_not_a_tier_report_v1() {
        let counts = [7u64, 5, 9];
        let bodies = synthetic_shard_bodies_v1(&counts, 2);
        for body in &bodies {
            let report = TtsS1ReplayShardReportV1::seal_v1(body.clone()).unwrap();
            let bytes = report.canonical_bytes_v1().unwrap();
            assert_eq!(
                decode_tts_s1_replay_shard_report_v1(&bytes).unwrap(),
                report
            );
            assert_eq!(report.schema, TTS_S1_REPLAY_SHARD_REPORT_SCHEMA_V1);
            assert_ne!(report.schema, TTS_S1_REPLAY_REPORT_SCHEMA_V1);
            // A shard report is not a tier report and vice versa.
            assert!(decode_tts_s1_replay_report_v1(&bytes).is_err());
        }
        let tier_bytes = synthetic_finalized_bytes_v1(&counts, 2);
        assert!(decode_tts_s1_replay_shard_report_v1(&tier_bytes).is_err());

        // A tampered digest is refused, exactly as the tier report's is.
        let mut report = TtsS1ReplayShardReportV1::seal_v1(bodies[0].clone()).unwrap();
        report.shard_report_sha256 = "ff".repeat(32);
        assert!(matches!(
            decode_tts_s1_replay_shard_report_v1(&report.canonical_bytes_v1().unwrap()),
            Err(TtsS1ReplayErrorV1::InvalidShardReport)
        ));
    }

    /// A fan-out wider than the corpus's contributing episodes is refused
    /// before anything runs, rather than publishing a shard that measured
    /// nothing.
    #[test]
    fn a_shard_with_no_episode_is_refused_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_v1(&scorer, 1);
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let identity = fixture_identity_v1();
        let directory = scratch_diagnostics_dir_v1("empty-shard");
        let error = replay_corpus_shard_body_v1(
            &scorer,
            &identity,
            &architecture,
            TtsS1CorpusCheckpointV1::from_identity_v1(&identity, &architecture),
            &corpus,
            KernelNativeSearchTierV1::T512,
            FIXTURE_SEED_BLOCK_ID_V1,
            MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1],
            FIXTURE_MAX_EPISODES_V1,
            None,
            TtsS1ShardSelectorV1::new_v1(1, 2).expect("a legal shard"),
            &directory,
        )
        .expect_err("a shard with no episode must be refused");
        assert!(matches!(
            error,
            TtsS1ReplayErrorV1::EmptyShard {
                shard_index: 1,
                shard_count: 2,
                planned_episodes: 1
            }
        ));
        assert_eq!(error.code_v1(), "tts_s1_replay_empty_shard");
        // The guard fires before the diagnostics writer opens, so nothing
        // at all was published for a shard that had nothing to do.
        assert!(!directory.join(TTS_S1_RESPONSE_LINES_FILE_V1).exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Replays one shard of the fixture corpus and returns its report.
    fn replay_shard_fixture_v1(
        scorer: &RunnerFixedScorerV1,
        corpus: &TtsS1CorpusManifestV1,
        tag: &str,
        shard: TtsS1ShardSelectorV1,
        directory: &Path,
    ) -> TtsS1ReplayShardReportV1 {
        let _ = tag;
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let identity = fixture_identity_v1();
        let body = replay_corpus_shard_body_v1(
            scorer,
            &identity,
            &architecture,
            TtsS1CorpusCheckpointV1::from_identity_v1(&identity, &architecture),
            corpus,
            KernelNativeSearchTierV1::T512,
            FIXTURE_SEED_BLOCK_ID_V1,
            MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1[FIXTURE_SEED_BLOCK_ID_V1],
            FIXTURE_MAX_EPISODES_V1,
            None,
            shard,
            directory,
        )
        .expect("the fixture shard replays");
        TtsS1ReplayShardReportV1::seal_v1(body).expect("the fixture shard report seals")
    }

    /// END TO END, t512, SHARDED: the same three-episode corpus replayed in
    /// one process and then split across K processes, with K = 1 and K = 3,
    /// and the merged tier report equal to the unsharded one.
    ///
    /// Equality is stated through [`strip_timing_fields_v1`], which is what
    /// "the same report" can mean between two real runs at all: the timings
    /// and everything derived from them (both views, the fitted curve, the
    /// projection, the chain links, the verdict, the digests, the
    /// diagnostics file hashes) are exactly what a re-run is allowed to
    /// change, and the S0 pattern this crate already uses for a
    /// bit-identical replay claim strips precisely those. What survives is
    /// the substantive claim: the same episodes, in the same order, with
    /// the same decisions, the same chosen actions and the same search
    /// products, whether one process or three produced them. The BYTE
    /// equality of the merge arithmetic itself, timings held fixed, is
    /// proved separately and cheaply by
    /// `a_merged_report_is_byte_identical_to_the_unsharded_one_v1`.
    ///
    /// IGNORED IN A DEBUG BUILD for the same reason as the other
    /// whole-episode tests, tripled: it replays three whole episodes three
    /// times over.
    ///
    ///     cargo test --release --lib native_tts_s1 -- --ignored
    #[test]
    #[ignore = "three whole-episode t512 replays of a three-episode corpus; minutes in release and far worse in debug: run with cargo test --release -- --ignored"]
    fn a_sharded_replay_merges_to_the_unsharded_report_v1() {
        let scorer = RunnerFixedScorerV1::new_v1();
        let corpus = fixture_corpus_episodes_v1(&scorer, 3, 2);
        assert_eq!(corpus.body.contributing_episode_count, 3);
        assert_eq!(
            corpus.body.contributing_episode_decisions,
            corpus
                .body
                .episodes
                .iter()
                .map(|episode| episode.decision_count)
                .sum::<u64>()
        );

        let unsharded = replay_fixture_v1(&scorer, &corpus, "shard-baseline")
            .canonical_bytes_v1()
            .unwrap();
        let unsharded_stripped = strip_timing_fields_v1(&unsharded).unwrap();

        for shard_count in [1u64, 3] {
            let shard_root = scratch_diagnostics_dir_v1(&format!("shard-run-{shard_count}"));
            let reports_dir = shard_root.join("reports");
            std::fs::create_dir_all(&reports_dir).expect("the shard report directory");
            for shard_index in 0..shard_count {
                let selector =
                    TtsS1ShardSelectorV1::new_v1(shard_index, shard_count).expect("a legal shard");
                let diagnostics = shard_root.join(format!("diagnostics-{shard_index}"));
                let report =
                    replay_shard_fixture_v1(&scorer, &corpus, "shard", selector, &diagnostics);
                // Exactly the episodes this index owns, and no other.
                assert_eq!(report.body.shard_index, shard_index);
                assert_eq!(report.body.shard_count, shard_count);
                assert_eq!(report.body.identity.episodes_replayed, 3);
                for episode in &report.body.episodes {
                    assert!(selector.owns_position_v1(episode.episode_position));
                    assert_eq!(episode.searched_decisions, episode.decision_count);
                    assert!(episode.protocol_micros_total > 0);
                }
                publish_tts_s1_replay_shard_report_v1(
                    &report,
                    &reports_dir.join(tts_s1_shard_report_file_name_v1(shard_index, shard_count)),
                )
                .expect("the shard report publishes");
            }

            let merged =
                merge_tts_s1_replay_shards_v1(&reports_dir, shard_count).expect("the shards merge");
            let merged_bytes = merged.canonical_bytes_v1().unwrap();
            // The merged document is a real, re-provable tier report.
            assert_eq!(
                decode_tts_s1_replay_report_v1(&merged_bytes).unwrap(),
                merged
            );
            assert_eq!(merged.body.episodes_replayed, 3);
            assert_eq!(
                merged.body.searched_decisions,
                corpus.body.contributing_episode_decisions
            );
            assert!(merged.body.replayed_whole_corpus);
            assert_eq!(merged.body.diagnostics_episode_files.len(), 3);
            assert_eq!(
                merged.body.compute_cap.latency_curve.observed_samples,
                merged.body.searched_decisions
            );

            assert_eq!(
                strip_timing_fields_v1(&merged_bytes).unwrap(),
                unsharded_stripped,
                "the merge of {shard_count} shards must be the unsharded tier report"
            );
            let _ = std::fs::remove_dir_all(&shard_root);
        }
    }
}
