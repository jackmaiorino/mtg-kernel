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
    TtsS1CorpusEpisodeV1, TtsS1CorpusErrorV1, TtsS1CorpusManifestV1, TtsS1DecisionScorerV1,
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
        config.max_episodes,
        config.limit_episodes,
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
    max_episodes: u64,
    limit_episodes: Option<u64>,
    diagnostics_directory: &Path,
) -> Result<TtsS1ReplayReportBodyV1, TtsS1ReplayErrorV1> {
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

    // WHOLE EPISODES, in ascending episode id. Every decision of a
    // contributing episode is reconstructed and searched, in order, not
    // only the stratified targets. That is not thoroughness for its own
    // sake: the production writer republishes the whole episode file after
    // every decision, so a decision's publication cost is a function of
    // every earlier searched decision in that episode. A replay that
    // searched only the sparse targets would publish short files and
    // measure a publication phase no panel ever pays.
    for episode in corpus
        .body
        .episodes
        .iter()
        .take(usize::try_from(planned_episodes).map_err(|_| TtsS1ReplayErrorV1::NoDecisions)?)
    {
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
            published_episodes.push((episode.episode_id, path, episode_record_indices));
        }
    }

    responses
        .flush()
        .map_err(|error| TtsS1ReplayErrorV1::Diagnostics(error.to_string()))?;
    drop(responses);

    let searched_decisions = records.len() as u64;
    if searched_decisions == 0 {
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

    // The two views. The whole-episode one is every searched decision and
    // is the verdict basis; the corpus-target one is the stratified subset
    // and is diagnostics only.
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
    let compute_cap = compute_cap_projection_v1(&curve_samples, &corpus.body.all_episode_decisions)
        .ok_or(TtsS1ReplayErrorV1::NoDecisions)?;
    let replayed_whole_corpus = planned_episodes == corpus_episode_count
        && corpus_targets_replayed == corpus_decision_count;
    let (verdict, verdict_reason) = verdict_v1(
        corpus_target_view.p99_protocol_ceiling_status,
        corpus_target_view.max_protocol_ceiling_status,
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
        corpus_episode_count,
        episodes_replayed: planned_episodes,
        searched_decisions,
        corpus_targets_replayed,
        max_episodes,
        replayed_whole_corpus,
        percentile_rule: TTS_S1_PERCENTILE_RULE_V1.to_owned(),
        verdict_view: TTS_S1_VERDICT_VIEW_V1.to_owned(),
        corpus_target_view,
        whole_episode_view,
        slo_micros: slo_micros_v1(),
        hard_timeout_micros: hard_timeout_micros_v1(),
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
            harvest.decisions.len() > take,
            "the fixture episode must be strictly longer than the target sample, so the \
             whole-episode replay is provably more than the targets: got {} for {take}",
            harvest.decisions.len()
        );
        // Spread the targets across the episode rather than clustering
        // them at its start, so the surface checks land at genuinely
        // different accumulated histories.
        let stride = (harvest.decisions.len() / take).max(1);
        let classification = harvest.classification;
        let actions = harvest.actions.clone();
        let environment_seed = harvest.decisions[0].coordinates.environment_seed;
        let all_decisions = harvest.into_decisions_with_action_sequences_v1();
        let episode_decisions = all_decisions.len() as u64;
        let decisions: Vec<_> = all_decisions
            .into_iter()
            .step_by(stride)
            .take(take)
            .collect();
        let candidate_count = decisions.len() as u64;
        let architecture = scorer.net.architecture_identity_v1().to_owned();
        let natural = classification == crate::rl::TerminalClassificationV1::Natural;
        TtsS1CorpusManifestV1::seal_v1(corpus_body_v1(
            TtsS1CorpusCheckpointV1::from_identity_v1(&fixture_identity_v1(), &architecture),
            FIXTURE_SEED_BLOCK_ID_V1,
            base_seed,
            1,
            FIXTURE_MAX_PHYSICAL_DECISIONS_V1,
            FIXTURE_MAX_POLICY_STEPS_V1,
            TtsS1CorpusSelectionV1 {
                decisions,
                episodes: vec![TtsS1CorpusEpisodeV1 {
                    episode_id: 0,
                    episode_base_seed: base_seed,
                    environment_seed,
                    decision_count: episode_decisions,
                    terminal_classification:
                        crate::native_tts_s1_corpus_v1::terminal_classification_tag_v1(
                            classification,
                        )
                        .to_owned(),
                    action_sequence: actions,
                }],
                candidate_count,
                natural_terminal_episode_count: u64::from(natural),
                truncated_episode_count: u64::from(!natural),
                episode_decisions: TtsS1EpisodeDecisionStatsV1::summarize_v1(&[episode_decisions])
                    .expect("one episode summarizes"),
                all_episode_decisions: TtsS1AllEpisodeDecisionStatsV1::summarize_v1(
                    &[episode_decisions],
                    u64::from(natural),
                    u64::from(!natural),
                )
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
}
