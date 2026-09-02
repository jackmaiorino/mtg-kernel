//! Outcome schema V4: per-decision diagnostics for the model-guided
//! (test-time search) selector.
//!
//! `LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5 (S0) names the
//! contents exactly: "outcome schema V3 recording requested and actual
//! transitions, simulations, ceiling status, decision ordinal,
//! search-authority digest, depth and terminal counts, and a
//! root-statistics digest; wrapper identity (core identity, tier, digests,
//! checkpoint manifest SHA-256) in the panel record", plus, from the S2
//! diagnostics list this schema has to be able to carry, "chosen-action
//! stability across two independent simulation-seed halves" and "visit
//! margin".
//!
//! # Why the wire identity says v4
//!
//! The sketch names "outcome schema V3", and the first cut of this writer
//! declared `.../v3` with schema version 3. The record shape has since
//! changed incompatibly twice: the in-record ceiling field was renamed to
//! say what it actually covers, and the wall-time group was restructured
//! around the outer protocol boundary. A reader written against the
//! original v3 shape cannot parse the current one, and a reader of the
//! current shape rejects the original, so continuing to call both "v3"
//! would make the version string a lie in both directions. The wire
//! identity is therefore [`MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V4`] /
//! [`MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V4`], and the module and
//! type names moved with it so that a `SearchDecisionRecordV4` is exactly
//! the record that declares schema 4.
//!
//! No v3 file was ever published: the writer's only production entry point
//! is the `--model-guided-search` scorer flag added in the same lineage,
//! S0 is engineering-only, and no wrapped game has been played. The v3
//! shape existed only inside this crate's tests, so no v3 reader is
//! provided and none is needed.
//!
//! # Four properties this writer is built around
//!
//! **Wall time is diagnostic and never affects the chosen action.** The
//! owner law is unconditional, so this module never returns a timing to
//! the selector, never compares one against a threshold in order to decide
//! anything, and records the SLO/hard-timeout comparison as a STATUS
//! STRING only. [`CeilingStatusV4`] is produced after the search has
//! already finished and its result is already fixed; there is no code path
//! by which a slow decision selects differently from a fast one. The
//! bit-identical replay test in the scorer proves the consequence: two
//! runs produce byte-identical diagnostics apart from the wall-time
//! fields, which are grouped into [`WallTimeV4`] precisely so a test (or
//! an auditor) can excise them in one move rather than by naming fields.
//!
//! **Atomic publication, by REPLACEMENT.** The whole episode file is
//! rewritten after every record and moved into place with a single
//! replacing move (`durable_move_publication_v2::replace_file_by_move_v2`,
//! which is `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`
//! on Windows), never appended to in place and never unlinked first. A
//! reader therefore only ever observes a complete, chain-valid file: a torn
//! write is impossible, there is no instant at which the path names nothing,
//! and a process killed mid-episode leaves the last successfully published
//! decision rather than a half line or no file at all. Rewriting is
//! affordable because an episode's record set is small (hundreds of
//! records of a few hundred bytes) while a single decision costs a full
//! tiered search, orders of magnitude more work than the rewrite.
//!
//! **Commit-after-publish, and stage recovery.** No chain state advances
//! until the publish has succeeded, so a failed publish is retryable and a
//! retry republishes the same decision at the same ordinals. The staging
//! file is cleared on entry (a leftover stage is stale by construction,
//! never a published artifact) and removed again on any failure, so one
//! interrupted publish cannot turn every later one into a permanent stage
//! collision. See `append_and_publish_v4` and `publish_atomically_v4`.
//!
//! **The whole client-visible request is accounted for, and every
//! completed decision is classifiable, including the last one.** A request
//! that searches a decision costs three consecutive synchronous phases,
//! and the client waits for all three:
//!
//! ```text
//! request receipt ---> record built ---> record published ---> response flushed
//!    |<-- decision_micros -->|<-- publish -->|<-- response tail -->|
//! ```
//!
//! Only the first is knowable while the record is being built. The
//! publication cannot be (measuring it requires publishing, which requires
//! the finished bytes) and the response tail cannot be (the record is
//! already on disk before the response line is serialized, written and
//! flushed, and before any other export runs). So each record carries its
//! PREDECESSOR's last two phases, in
//! `wall_time.previous_record_publish_micros` and
//! `wall_time.previous_record_response_micros`, and the protocol verdict
//! is a chain-level read: [`episode_decision_ceilings_v4`] sums all three.
//! Charging only the first two would let a slow export or a slow stdout
//! push a request past the 4 s SLO or the 20 s hard timeout while the
//! record still classified it as comfortably inside.
//!
//! The successor rule used to leave the FINAL decision of every episode
//! unclassifiable, because nothing followed it to report its publication:
//! one systematically dropped sample per game, and always the same
//! decision (the one that ended the game). The
//! [`EpisodeFooterRecordV4`] closes it. An episode is CLOSED by a footer
//! on terminal, on replacement by a new reset, and on orderly process
//! exit; the footer is an ordinary hash-chained record carrying the same
//! two predecessor fields, so the reconstruction rule needs no special
//! case: the successor of the last decision is simply the footer.
//! [`verify_episode_chain_v4`] accepts a footer only as the terminal
//! record and rejects anything after it, so a file that ends in a footer
//! is a complete episode by construction and every decision in it has a
//! protocol verdict.
//!
//! # Hash chaining across records within an episode
//!
//! Each record carries `previous_record_sha256`, the SHA-256 of the
//! previous record's serialized line INCLUDING the newline the file
//! actually contains; the header carries sixty-four zeros. Chaining by
//! predecessor rather than by a self-referential `record_sha256` avoids
//! the usual "hash the record with its own hash field blanked"
//! contortion, which is easy to implement subtly differently in the
//! verifier than in the writer. The footer additionally carries
//! `episode_content_sha256`, one digest over every byte that precedes it,
//! so a complete episode has a single value that pins the whole file.
//! [`verify_episode_chain_v4`] is the verifier, and the writer's own tests
//! run it against real published bytes.
//!
//! # Canonical bytes
//!
//! Records serialize through `serde_json::to_string` on structs with a
//! fixed field order and no maps, so a line's bytes are a pure function of
//! its field values. This matches the JSONL export writers already in
//! `native_checkpoint_shadow_stdio_v1`; no separate canonicalization pass
//! is needed or performed.

use crate::durable_move_publication_v2::replace_file_by_move_v2;
use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, DurableFileExpectationV1,
};
use crate::model_guided_search_core_v1::{
    ModelGuidedSearchDecisionV1, ModelGuidedSearchLeafCensusV1,
};
use crate::rl::PlayerSeatV1;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V4: &str =
    "mtg-kernel-model-guided-search-outcome-jsonl/v4";
pub const MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V4: u32 = 4;

/// The three record kinds, as they appear on the wire. A record whose
/// `record_kind` is none of these is rejected by the verifier rather than
/// skipped: an unknown kind in a chained file is either a forward-version
/// file this reader must not pretend to understand or a forgery.
pub const EPISODE_HEADER_RECORD_KIND_V4: &str = "episode_header";
pub const SEARCH_DECISION_RECORD_KIND_V4: &str = "search_decision";
pub const EPISODE_FOOTER_RECORD_KIND_V4: &str = "episode_footer";

/// The genesis chain link: sixty-four zeros, never a real SHA-256 of any
/// record.
pub const MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V4: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Pre-registered per-decision wall-time service level, in seconds (sketch
/// Section 4: "p99 per-decision wall time < 4.0 s on the panel host").
/// Recorded, never enforced.
pub const MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4: f64 = 4.0;

/// Pre-registered hard protocol timeout, in seconds (sketch Section 4:
/// "Hard protocol timeout: 20.0 s"). Also recorded, never enforced HERE:
/// the sketch makes a timeout inside a formal game "a product failure of
/// that panel", which is a panel-protocol consequence, not something a
/// diagnostics writer may act on. Aborting a search on a clock would make
/// the chosen action a function of wall time, which the owner law forbids
/// outright.
pub const MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4: f64 = 20.0;

/// Where one decision's elapsed time landed against the two pre-registered
/// ceilings. A recorded observation with no control-flow consequence
/// anywhere in this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CeilingStatusV4 {
    /// At or under the 4.0 s SLO.
    WithinSlo,
    /// Over the SLO, under the 20.0 s hard timeout.
    SloExceeded,
    /// At or over the 20.0 s hard timeout. Inside a formal panel this is a
    /// product failure of that panel; the search still ran to its full
    /// pre-registered transition budget and its result still stands,
    /// because a budget, not a clock, is what bounds it.
    HardTimeoutExceeded,
}

impl CeilingStatusV4 {
    pub fn classify_v4(elapsed_seconds: f64) -> Self {
        if elapsed_seconds >= MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4 {
            Self::HardTimeoutExceeded
        } else if elapsed_seconds > MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4 {
            Self::SloExceeded
        } else {
            Self::WithinSlo
        }
    }
}

/// Which protocol request opened the measured window for a decision.
///
/// A `reset` window also contains the one-off session construction (deck
/// build, opening library order, first encode), which a `step` window does
/// not, so the two are not drawn from the same latency population. The
/// record says which it is rather than leaving an analyst to infer it from
/// `decision_ordinal == 0`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolRequestKindV4 {
    Reset,
    Step,
}

/// Why an episode was closed. Recorded on the footer so a reader can tell
/// a game that ended from a run that was cut short.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeCloseReasonV4 {
    /// The episode reached a terminal state.
    EpisodeTerminal,
    /// A new reset replaced this episode before it terminated.
    EpisodeReplaced,
    /// The scorer's serving loop reached end of input with this episode
    /// still open.
    ProcessExit,
}

/// Every wall-time field on a decision record, grouped into one struct so
/// a replay test can excise timing in a single move. Nothing outside this
/// struct varies between two runs of the same decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallTimeV4 {
    /// The full-budget search alone.
    pub full_search_micros: u64,
    pub stability_half_a_micros: u64,
    pub stability_half_b_micros: u64,
    /// The SEARCH-ONLY timer: the full-budget search plus the two
    /// diagnostic stability halves, which run synchronously inside the
    /// same call. Kept as its own field so the search cost stays readable
    /// independently of the protocol cost around it.
    pub search_micros: u64,
    /// The PROTOCOL timer, minus this record's own publication.
    ///
    /// Measured from request receipt at the scorer's outer decision
    /// boundary, so it covers packet encoding, tensorization, model
    /// scoring, policy sampling, the search, the stability halves, record
    /// construction, and writer bookkeeping: everything the client waits
    /// for EXCEPT the publication of this very record, which by
    /// construction cannot be known while the record is still being built.
    ///
    /// The earlier shape started its clock after the policy sample and
    /// stopped it before record construction, so a request sitting near
    /// the 4 s SLO or the 20 s hard timeout could be recorded as
    /// comfortably inside it.
    pub decision_micros: u64,
    /// How long the writer spent publishing the record immediately BEFORE
    /// this one.
    ///
    /// Publication is synchronous and completes before the client gets its
    /// response, so it is part of the protocol latency the panel host
    /// pays. A record cannot carry its own publish time (measuring it
    /// requires publishing, and publishing requires the finished bytes),
    /// so each record carries its predecessor's instead and a decision's
    /// true latency is read off the chain. The episode footer is a record
    /// like any other for this purpose, which is what makes the final
    /// decision classifiable too. See [`episode_decision_ceilings_v4`],
    /// which is the only correct way to classify a decision's protocol
    /// ceiling.
    ///
    /// Writer-assigned, like every other chain field: a caller that could
    /// set this could understate the latency it is being measured on.
    pub previous_record_publish_micros: u64,
    /// The RESPONSE TAIL of the request that published the record
    /// immediately before this one: everything synchronous after that
    /// record was published and before the client's response line was
    /// written and flushed. Later exports, response serialization, the
    /// stdout write, and the flush all live here.
    ///
    /// Carried by the successor for the same reason the publish time is:
    /// the record is already on disk before any of it happens. Without it
    /// a slow export or a slow output path could push the request the
    /// client actually waited on past a pre-registered ceiling while the
    /// classification still reported it inside.
    ///
    /// Writer-assigned. Zero means "the serving loop never closed the
    /// outer boundary for that request", which only happens when a caller
    /// drives `handle_line_v1` directly instead of running the loop; the
    /// protocol verdict is then a lower bound rather than wrong.
    pub previous_record_response_micros: u64,
}

/// The footer's timing surface. Kept in a field literally named
/// `wall_time`, like the decision record's, so one normalization rule
/// ("replace the `wall_time` member") covers every record kind and cannot
/// silently miss a timing field on a kind it did not know about.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FooterWallTimeV4 {
    /// The publication of the record immediately before this footer. For
    /// an episode that searched at least one decision, that is the last
    /// decision's publication: the sample the old shape dropped.
    pub previous_record_publish_micros: u64,
    /// The response tail of the request that published the record
    /// immediately before this footer. Same field, same meaning, and same
    /// reason as on a decision record: the footer is just the last
    /// decision's successor.
    pub previous_record_response_micros: u64,
}

/// Wrapper identity, carried once per episode in the header record (sketch
/// Section 5: "wrapper identity (core identity, tier, digests, checkpoint
/// manifest SHA-256) in the panel record").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrapperIdentityV4 {
    pub core_algorithm_identity: String,
    pub authority_kind: String,
    pub authority_schema: String,
    pub node_key_identity: String,
    pub seed_domain: String,
    pub tier: String,
    pub transition_budget: u32,
    pub policy_step_depth_cap: u16,
    pub seed_block_id: u64,
    pub action_seed_u64_hex: String,
    pub search_authority_digest_sha256: String,
    /// The lineage the authority binds: the identity of the checkpoint
    /// actually loaded, not the generic authority-kind string every
    /// Store-backed checkpoint shares.
    pub checkpoint_lineage_id: String,
    /// The architecture identity of the net that actually runs the search
    /// forward, read off the loaded net.
    pub net_architecture_identity: String,
    pub puct_prior_quantization_contract_sha256: String,
    pub value_quantization_contract_sha256: String,
    pub forward_determinism_build_identity: String,
    pub value_head_domain: String,
    pub checkpoint_manifest_sha256: String,
    pub checkpoint_model_parameter_sha256: String,
    pub engine_commit: String,
}

/// Chosen-action agreement between the two independent simulation-seed
/// halves. The full-budget result is the chosen action; these two are
/// recorded so a later analysis can measure how settled that choice was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StabilityV4 {
    pub half_a_selected_index: u32,
    pub half_b_selected_index: u32,
    pub half_transition_budget: u32,
    /// The two halves agreed with each other.
    pub halves_agree: bool,
    /// Both halves agreed with the full-budget chosen action.
    pub halves_agree_with_full_budget: bool,
}

/// One episode-opening record, then one record per searched decision, then
/// one closing footer. A serde internally-tagged enum is deliberately
/// avoided in favor of an explicit `record_kind` field so the JSON key
/// order stays fully under these structs' own control.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeHeaderRecordV4 {
    pub contract: String,
    pub schema_version: u32,
    pub record_kind: String,
    pub record_ordinal: u64,
    pub previous_record_sha256: String,
    pub episode_id: u64,
    pub base_seed_u64_hex: String,
    pub candidate_seat: PlayerSeatV1,
    pub decision_slo_seconds: f64,
    pub decision_hard_timeout_seconds: f64,
    pub wrapper_identity: WrapperIdentityV4,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchDecisionRecordV4 {
    pub contract: String,
    pub schema_version: u32,
    pub record_kind: String,
    pub record_ordinal: u64,
    pub previous_record_sha256: String,
    pub episode_id: u64,
    /// Ordinal of this SEARCHED decision within the episode, counted from
    /// zero. Distinct from `record_ordinal` (which also counts the header)
    /// and from `step` (which counts every policy step, including the ones
    /// the opponent seat owns and this selector never searches).
    pub decision_ordinal: u64,
    pub step: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub acting_player: PlayerSeatV1,
    pub legal_action_count: u32,
    pub search_authority_digest_sha256: String,
    pub requested_transitions: u32,
    pub actual_transitions: u32,
    pub simulations: u32,
    pub tree_node_count: u32,
    pub leaf_census: ModelGuidedSearchLeafCensusV1,
    pub root_statistics_digest_sha256: String,
    pub chosen_action_index: u32,
    /// Visits of the chosen root action minus visits of the runner-up.
    /// Zero when the top two are tied.
    pub visit_margin: u32,
    /// The policy sample this decision would have played without the
    /// wrapper, recorded so the raw-policy diagnostic and the wrapped
    /// choice can be compared on the same decision without a second run.
    pub policy_sample_index: u32,
    pub search_overrode_policy_sample: bool,
    /// `None` when the stability halves were disabled for this run. A
    /// null here and `stability_halves_enabled: false` say the same thing
    /// two ways on purpose: a reader that only knows one of the fields
    /// still cannot mistake "halves not run" for "halves disagreed".
    pub stability: Option<StabilityV4>,
    /// Whether the two diagnostic stability halves ran for this decision.
    /// They run synchronously inside the decision, so this also says
    /// whether `wall_time.search_micros` is the product's own cost or the
    /// diagnostics-inclusive one.
    pub stability_halves_enabled: bool,
    /// Which request opened the measured protocol window; see
    /// [`ProtocolRequestKindV4`].
    pub protocol_request_kind: ProtocolRequestKindV4,
    /// Classification of `wall_time.search_micros` ALONE, i.e. of the
    /// search and (when enabled) its stability halves. It deliberately
    /// does NOT include the protocol work around the search nor this
    /// record's own publication.
    ///
    /// Named `search_ceiling_status` rather than `ceiling_status` for
    /// exactly that reason: a field called `ceiling_status` that omits
    /// synchronous phases of the very latency it claims to bound is a
    /// field that will be misread, and was. For the protocol-latency
    /// verdict use [`episode_decision_ceilings_v4`], which adds the
    /// protocol window and the publish time the successor record reports.
    pub search_ceiling_status: CeilingStatusV4,
    /// DIAGNOSTIC ONLY. Excluded from every determinism comparison; see
    /// the module docs.
    pub wall_time: WallTimeV4,
}

/// The record that CLOSES an episode.
///
/// Written on terminal, on replacement by a new reset, and on orderly
/// process exit. Two things only it can carry: the publication time of the
/// last decision record (nothing else follows that record to observe it),
/// and one digest over the whole episode's bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeFooterRecordV4 {
    pub contract: String,
    pub schema_version: u32,
    pub record_kind: String,
    pub record_ordinal: u64,
    pub previous_record_sha256: String,
    pub episode_id: u64,
    pub close_reason: EpisodeCloseReasonV4,
    /// How many `search_decision` records this episode published. A reader
    /// that finds a different count has a truncated or padded file.
    pub decision_record_count: u64,
    /// SHA-256 over every byte of the episode file that PRECEDES this
    /// footer's own line: the header, every decision record, and their
    /// newlines. One value that pins a complete episode, checked by
    /// [`verify_episode_chain_v4`].
    pub episode_content_sha256: String,
    pub wall_time: FooterWallTimeV4,
}

/// SHA-256 over the root action statistics, in flat-action-index order.
/// Committing to visits, value sums, and means (rather than to the chosen
/// index alone) is what makes this a fingerprint of the SEARCH, not just
/// of its verdict: two runs that pick the same action for different
/// reasons are distinguishable.
pub fn root_statistics_digest_v4(decision: &ModelGuidedSearchDecisionV1) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update((decision.root_action_stats.len() as u64).to_le_bytes());
    for stat in &decision.root_action_stats {
        hasher.update(stat.flat_action_index.to_le_bytes());
        hasher.update(stat.visits.to_le_bytes());
        hasher.update(stat.value_sum.to_le_bytes());
        hasher.update(stat.mean_value.to_le_bytes());
    }
    hasher.update(decision.selected_index.to_le_bytes());
    hasher.update(decision.transitions_used.to_le_bytes());
    hasher.update(decision.simulations.to_le_bytes());
    hasher.update(decision.tree_node_count.to_le_bytes());
    hasher.finalize().into()
}

/// Visits of the chosen root action minus visits of the best OTHER root
/// action. Saturating, so a tie at the top reads as zero rather than
/// wrapping.
pub fn visit_margin_v4(decision: &ModelGuidedSearchDecisionV1) -> u32 {
    let chosen = decision
        .root_action_stats
        .iter()
        .find(|stat| stat.flat_action_index == decision.selected_index)
        .map_or(0, |stat| stat.visits);
    let runner_up = decision
        .root_action_stats
        .iter()
        .filter(|stat| stat.flat_action_index != decision.selected_index)
        .map(|stat| stat.visits)
        .max()
        .unwrap_or(0);
    chosen.saturating_sub(runner_up)
}

pub fn lower_hex_sha256_v4(digest: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The chain link a record contributes: SHA-256 over its serialized line
/// PLUS the newline the file actually contains, so the digest covers
/// exactly the bytes on disk and a verifier cannot disagree with the
/// writer about trailing whitespace.
///
/// `pub` because the chain covers the PUBLISHED bytes, wall-time fields
/// included, so a determinism comparison that neutralizes wall time must
/// re-derive the chain over its own neutralized lines rather than compare
/// the published links (which legitimately differ between two runs of the
/// same decision). The scorer's replay test does exactly that.
pub fn record_chain_link_v4(line: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(line.as_bytes());
    hasher.update(b"\n");
    hasher.finalize().into()
}

/// One decision's latency accounting, reconstructed from the chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionCeilingV4 {
    pub decision_ordinal: u64,
    /// `wall_time.search_micros`: the search and its halves.
    pub search_micros: u64,
    /// `wall_time.decision_micros`: request receipt through record
    /// construction, excluding this record's own publication.
    pub decision_micros: u64,
    /// The synchronous publication of THIS decision's own record, read
    /// from the successor record's `previous_record_publish_micros`. The
    /// successor is the next decision, or the episode footer for the last
    /// one.
    ///
    /// `None` only for a decision with no successor at all, which means an
    /// episode still being written or one whose process was killed before
    /// it could publish a footer. A file that ends in a footer never has
    /// one.
    pub publish_micros: Option<u64>,
    /// The response tail of the request that answered with THIS decision:
    /// exports, response serialization, the stdout write and the flush.
    /// Read from the same successor record. `None` under the same
    /// condition as `publish_micros`.
    pub response_micros: Option<u64>,
    /// `decision_micros + publish_micros + response_micros`: the whole
    /// synchronous cost the client waited for. `None` exactly when
    /// `publish_micros` is `None`.
    pub protocol_micros: Option<u64>,
    /// What the record itself claims, over the search alone.
    pub search_ceiling_status: CeilingStatusV4,
    /// The protocol verdict, classified from `protocol_micros`. `None`
    /// exactly when `publish_micros` is `None`.
    pub protocol_ceiling_status: Option<CeilingStatusV4>,
}

/// Reconstructs every decision's TRUE per-decision protocol latency from a
/// published episode file, and classifies it.
///
/// # The classification rule
///
/// A client waiting on decision `n` waits from the moment the scorer read
/// its request line until the moment its response line has been written
/// and flushed. That spans three consecutive synchronous phases, and all
/// three are charged:
///
/// ```text
/// latency(n)  = record[n].wall_time.decision_micros
///             + record[n + 1].wall_time.previous_record_publish_micros
///             + record[n + 1].wall_time.previous_record_response_micros
/// status(n)   = CeilingStatusV4::classify_v4(latency(n) / 1e6)
/// ```
///
/// where `record[n + 1]` is the immediately following record in the file,
/// decision or footer. A record can carry neither its own publish time
/// (measuring it requires publishing, and publishing requires the finished
/// bytes) nor its own response tail (the record is already on disk before
/// the response is serialized, written and flushed), so the successor
/// carries both and the verdict is a chain-level read. The footer exists
/// so that the last decision has a successor too; in a closed episode this
/// function returns a protocol status for every decision.
/// `SearchDecisionRecordV4::search_ceiling_status` deliberately covers the
/// search alone and is named to say so.
///
/// The episode's chain is verified first, so a tampered file cannot
/// produce a latency report.
pub fn episode_decision_ceilings_v4(bytes: &[u8]) -> Result<Vec<DecisionCeilingV4>, String> {
    verify_episode_chain_v4(bytes)?;
    let text = std::str::from_utf8(bytes).map_err(|_| "episode file is not UTF-8".to_owned())?;
    let values: Vec<serde_json::Value> = text
        .lines()
        .map(|line| {
            serde_json::from_str(line).map_err(|error| format!("record is not JSON: {error}"))
        })
        .collect::<Result<_, _>>()?;
    // The successor record reports BOTH phases the predecessor could not
    // observe about itself: its publication and the response tail that
    // followed it. They are read as one pair so a successor that reports
    // only half of the interval cannot silently produce a verdict that
    // omits the other half.
    let successor_tail_of = |index: usize| -> Option<(u64, u64)> {
        let wall = values.get(index + 1)?.get("wall_time")?;
        Some((
            wall.get("previous_record_publish_micros")?.as_u64()?,
            wall.get("previous_record_response_micros")?.as_u64()?,
        ))
    };
    let mut out = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if value.get("record_kind").and_then(|kind| kind.as_str())
            != Some(SEARCH_DECISION_RECORD_KIND_V4)
        {
            continue;
        }
        let record: SearchDecisionRecordV4 = serde_json::from_value(value.clone())
            .map_err(|error| format!("decision record does not match the schema: {error}"))?;
        let tail = successor_tail_of(index);
        let protocol_micros = tail.map(|(publish, response)| {
            record
                .wall_time
                .decision_micros
                .saturating_add(publish)
                .saturating_add(response)
        });
        out.push(DecisionCeilingV4 {
            decision_ordinal: record.decision_ordinal,
            search_micros: record.wall_time.search_micros,
            decision_micros: record.wall_time.decision_micros,
            publish_micros: tail.map(|(publish, _)| publish),
            response_micros: tail.map(|(_, response)| response),
            protocol_micros,
            search_ceiling_status: record.search_ceiling_status,
            protocol_ceiling_status: protocol_micros
                .map(|micros| CeilingStatusV4::classify_v4(micros as f64 / 1_000_000.0)),
        });
    }
    Ok(out)
}

/// Re-derives the chain over an already-published episode file and returns
/// the record count, or an error naming the first broken link. Used by
/// this module's tests and available to any auditor of a published file.
///
/// Beyond the links, the STRUCTURE is enforced, because the footer only
/// means "this episode is complete" if nothing can follow it:
///
/// - record 0 is the episode header, and no later record is a header;
/// - a footer, if present, is the LAST record: a decision (or anything
///   else) after a footer is rejected;
/// - a record kind this version does not know is rejected rather than
///   skipped;
/// - a footer's `episode_content_sha256` must equal the SHA-256 of every
///   byte before it, and its `decision_record_count` must equal the number
///   of decision records actually present.
pub fn verify_episode_chain_v4(bytes: &[u8]) -> Result<usize, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "episode file is not UTF-8".to_owned())?;
    if !text.ends_with('\n') || text.contains('\r') {
        return Err("episode file must be LF-terminated JSONL".to_owned());
    }
    let mut expected_previous = MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V4.to_owned();
    let mut count = 0usize;
    let mut decision_count = 0u64;
    let mut footer_seen = false;
    let mut content_len = 0usize;
    for (index, line) in text.lines().enumerate() {
        if footer_seen {
            return Err(format!(
                "record {index} follows the episode footer, which must be terminal"
            ));
        }
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|_| format!("record {index} is not JSON"))?;
        let kind = value
            .get("record_kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("record {index} has no record_kind"))?;
        match kind {
            EPISODE_HEADER_RECORD_KIND_V4 if index == 0 => {}
            EPISODE_HEADER_RECORD_KIND_V4 => {
                return Err(format!("record {index} is a second episode header"));
            }
            SEARCH_DECISION_RECORD_KIND_V4 | EPISODE_FOOTER_RECORD_KIND_V4 if index == 0 => {
                return Err(format!("record {index} precedes the episode header"));
            }
            SEARCH_DECISION_RECORD_KIND_V4 => decision_count += 1,
            EPISODE_FOOTER_RECORD_KIND_V4 => footer_seen = true,
            other => return Err(format!("record {index} has unknown record_kind {other}")),
        }
        let actual_previous = value
            .get("previous_record_sha256")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("record {index} has no previous_record_sha256"))?;
        if actual_previous != expected_previous {
            return Err(format!("record {index} breaks the chain"));
        }
        let ordinal = value
            .get("record_ordinal")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("record {index} has no record_ordinal"))?;
        if ordinal != index as u64 {
            return Err(format!("record {index} has ordinal {ordinal}"));
        }
        if footer_seen {
            let footer: EpisodeFooterRecordV4 = serde_json::from_value(value.clone())
                .map_err(|error| format!("record {index} is not a valid footer: {error}"))?;
            if footer.episode_content_sha256
                != lower_hex_sha256_v4(episode_content_digest_v4(&bytes[..content_len]))
            {
                return Err(format!(
                    "record {index} footer does not commit to the episode's bytes"
                ));
            }
            if footer.decision_record_count != decision_count {
                return Err(format!(
                    "record {index} footer claims {} decisions, the file has {decision_count}",
                    footer.decision_record_count
                ));
            }
        }
        content_len += line.len() + 1;
        expected_previous = lower_hex_sha256_v4(record_chain_link_v4(line));
        count += 1;
    }
    Ok(count)
}

/// SHA-256 over the episode bytes a footer commits to: everything
/// published before the footer's own line, newlines included.
pub fn episode_content_digest_v4(bytes_before_footer: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes_before_footer);
    hasher.finalize().into()
}

struct OpenEpisodeV4 {
    episode_id: u64,
    path: PathBuf,
    lines: Vec<String>,
    previous_record_sha256: String,
    next_record_ordinal: u64,
    next_decision_ordinal: u64,
    /// Wall time the most recent successful publish took, carried into the
    /// NEXT record so a decision's synchronous publication cost is
    /// recoverable from the chain. Zero before anything has been
    /// published.
    last_publish_micros: u64,
    /// The response tail measured after the most recent publish, carried
    /// into the NEXT record for the same reason. Filled in by
    /// [`ModelGuidedSearchOutcomeWriterV4::note_request_completed_v4`],
    /// which the serving loop calls once the response has been flushed.
    last_response_micros: u64,
    /// `wall_time.decision_micros` of the most recently published record,
    /// or zero for a header or footer. Subtracted (with the publish time)
    /// from the whole request to isolate the response tail.
    last_record_decision_micros: u64,
    /// True between a successful publish and the response boundary that
    /// follows it. Without it, a request that published nothing (a
    /// `score_current`, a rejected request) would overwrite an already
    /// measured tail with an interval that belongs to no record.
    response_accounting_pending: bool,
}

/// Per-episode JSONL diagnostics writer: one file per episode in the
/// configured directory, republished atomically after every record.
pub struct ModelGuidedSearchOutcomeWriterV4 {
    directory: PathBuf,
    open: Option<OpenEpisodeV4>,
    /// Test-only fault injection: an artificial delay inside the measured
    /// publication window, so the publication-inclusive ceiling
    /// classification can be exercised without a genuinely slow disk.
    #[cfg(test)]
    publish_delay_for_test_v4: Option<std::time::Duration>,
}

impl ModelGuidedSearchOutcomeWriterV4 {
    /// Opens the writer over an EXISTING, writable directory. The
    /// directory is deliberately not created: silently minting a deep path
    /// is how a typo'd diagnostics flag ends up writing a panel's
    /// diagnostics somewhere nobody looks.
    pub fn open_directory_v4(directory: PathBuf) -> io::Result<Self> {
        if !fs::metadata(&directory)?.is_dir() {
            return Err(io::Error::other(
                "search diagnostics path is not a directory",
            ));
        }
        Ok(Self {
            directory,
            open: None,
            #[cfg(test)]
            publish_delay_for_test_v4: None,
        })
    }

    /// Test-only: injects an artificial delay INSIDE the measured
    /// publication window, so a test can drive the publication-inclusive
    /// ceiling classification past a pre-registered boundary without
    /// needing a genuinely slow disk.
    ///
    /// `pub(crate)` so the scorer's tests can prove the thing that matters
    /// most about this whole module: a decision measured as slow chooses
    /// exactly the action it chose when it was measured as fast.
    #[cfg(test)]
    pub(crate) fn set_publish_delay_for_test_v4(&mut self, delay: std::time::Duration) {
        self.publish_delay_for_test_v4 = Some(delay);
    }

    pub fn directory_v4(&self) -> &Path {
        &self.directory
    }

    /// Path this writer uses for one episode. A pure function of (episode
    /// id, base seed), so a replay writes the same file name.
    pub fn episode_path_v4(&self, episode_id: u64, base_seed: u64) -> PathBuf {
        self.directory.join(format!(
            "model_guided_search_outcome_v4_episode_{episode_id:020}_seed_{base_seed:016x}.jsonl"
        ))
    }

    pub fn open_episode_id_v4(&self) -> Option<u64> {
        self.open.as_ref().map(|episode| episode.episode_id)
    }

    pub fn has_open_episode_v4(&self) -> bool {
        self.open.is_some()
    }

    /// Opens a new episode and publishes its header immediately, so an
    /// episode that searches no decision at all still leaves an auditable
    /// file recording which wrapper was configured.
    ///
    /// FAILS CLOSED if an episode is still open. The previous episode's
    /// footer is what makes its last decision classifiable and what marks
    /// its file complete, so silently abandoning an open episode here
    /// would reintroduce the dropped sample the footer exists to prevent.
    /// The caller closes first, with the reason it knows and this writer
    /// does not.
    pub fn begin_episode_v4(
        &mut self,
        episode_id: u64,
        base_seed: u64,
        candidate_seat: PlayerSeatV1,
        wrapper_identity: WrapperIdentityV4,
    ) -> io::Result<()> {
        if self.open.is_some() {
            return Err(io::Error::other(
                "a search-diagnostics episode is still open and must be closed with a footer first",
            ));
        }
        let path = self.episode_path_v4(episode_id, base_seed);
        let header = EpisodeHeaderRecordV4 {
            contract: MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V4.to_owned(),
            schema_version: MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V4,
            record_kind: EPISODE_HEADER_RECORD_KIND_V4.to_owned(),
            record_ordinal: 0,
            previous_record_sha256: MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V4.to_owned(),
            episode_id,
            base_seed_u64_hex: format!("{base_seed:016x}"),
            candidate_seat,
            decision_slo_seconds: MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V4,
            decision_hard_timeout_seconds: MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V4,
            wrapper_identity,
        };
        let line = serde_json::to_string(&header).map_err(io::Error::other)?;
        self.open = Some(OpenEpisodeV4 {
            episode_id,
            path,
            lines: Vec::new(),
            previous_record_sha256: MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V4.to_owned(),
            next_record_ordinal: 0,
            next_decision_ordinal: 0,
            last_publish_micros: 0,
            last_response_micros: 0,
            last_record_decision_micros: 0,
            response_accounting_pending: false,
        });
        let published = self.append_and_publish_v4(line, false, 0);
        if published.is_err() {
            // The header never reached disk, so there is no episode to
            // close and no file for a footer to belong to. Dropping the
            // in-memory episode here is what keeps `begin` retryable.
            self.open = None;
        }
        published
    }

    /// The next decision ordinal this writer will assign, so the selector
    /// can carry it in an error report without guessing.
    pub fn next_decision_ordinal_v4(&self) -> io::Result<u64> {
        self.open
            .as_ref()
            .map(|episode| episode.next_decision_ordinal)
            .ok_or_else(|| io::Error::other("no open search-diagnostics episode"))
    }

    /// Appends one decision record. `contract`, `schema_version`,
    /// `record_kind`, `record_ordinal`, `decision_ordinal`, and
    /// `previous_record_sha256` are assigned HERE rather than accepted
    /// from the caller: they are chain state, and a caller that could set
    /// them could forge a chain.
    pub fn write_decision_v4(&mut self, mut record: SearchDecisionRecordV4) -> io::Result<()> {
        let (ordinal, decision_ordinal, previous, last_publish_micros, last_response_micros) = {
            let episode = self
                .open
                .as_ref()
                .ok_or_else(|| io::Error::other("no open search-diagnostics episode"))?;
            if record.episode_id != episode.episode_id {
                return Err(io::Error::other(
                    "decision record episode does not match the open episode",
                ));
            }
            (
                episode.next_record_ordinal,
                episode.next_decision_ordinal,
                episode.previous_record_sha256.clone(),
                episode.last_publish_micros,
                episode.last_response_micros,
            )
        };
        record.contract = MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V4.to_owned();
        record.schema_version = MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V4;
        record.record_kind = SEARCH_DECISION_RECORD_KIND_V4.to_owned();
        record.record_ordinal = ordinal;
        record.decision_ordinal = decision_ordinal;
        record.previous_record_sha256 = previous;
        record.wall_time.previous_record_publish_micros = last_publish_micros;
        record.wall_time.previous_record_response_micros = last_response_micros;
        let decision_micros = record.wall_time.decision_micros;
        let line = serde_json::to_string(&record).map_err(io::Error::other)?;
        self.append_and_publish_v4(line, true, decision_micros)
    }

    /// Closes the OUTER request boundary: `request_micros` is the whole
    /// interval from request receipt to the moment the response line was
    /// written and flushed.
    ///
    /// The tail that belongs to the record published during that request
    /// is what is left after subtracting the two intervals already
    /// accounted for, the record's own `decision_micros` and its
    /// publication, so exports, response serialization, the write and the
    /// flush all land here. It is stored and carried into the NEXT record
    /// (or the footer), because the record it belongs to was on disk
    /// before any of it happened.
    ///
    /// A request that published no record (a `score_current`, a rejected
    /// request, a failed publish) is IGNORED: `response_accounting_pending`
    /// is false, and attributing an unrelated interval to an already
    /// measured record would be worse than measuring nothing. For the same
    /// reason a second call for the same publish is a no-op.
    pub fn note_request_completed_v4(&mut self, request_micros: u64) {
        let Some(episode) = self.open.as_mut() else {
            return;
        };
        if !episode.response_accounting_pending {
            return;
        }
        episode.last_response_micros = request_micros
            .saturating_sub(episode.last_record_decision_micros)
            .saturating_sub(episode.last_publish_micros);
        episode.response_accounting_pending = false;
    }

    /// CLOSES the open episode with a footer, then forgets it.
    ///
    /// The footer is what makes the last decision's publication
    /// observable: nothing else follows that record, so without this the
    /// final search of every episode has no protocol verdict at all, which
    /// is one systematically dropped sample per game and always the same
    /// one. It also commits, in a single digest, to every byte the episode
    /// published.
    ///
    /// Commit-after-publish, like every other record: a failed close
    /// leaves the episode open and retryable rather than silently
    /// discarding it.
    pub fn close_episode_v4(&mut self, close_reason: EpisodeCloseReasonV4) -> io::Result<()> {
        let footer = {
            let episode = self
                .open
                .as_ref()
                .ok_or_else(|| io::Error::other("no open search-diagnostics episode"))?;
            let mut content = Vec::new();
            for line in &episode.lines {
                content.extend_from_slice(line.as_bytes());
                content.push(b'\n');
            }
            EpisodeFooterRecordV4 {
                contract: MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V4.to_owned(),
                schema_version: MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V4,
                record_kind: EPISODE_FOOTER_RECORD_KIND_V4.to_owned(),
                record_ordinal: episode.next_record_ordinal,
                previous_record_sha256: episode.previous_record_sha256.clone(),
                episode_id: episode.episode_id,
                close_reason,
                decision_record_count: episode.next_decision_ordinal,
                episode_content_sha256: lower_hex_sha256_v4(episode_content_digest_v4(&content)),
                wall_time: FooterWallTimeV4 {
                    previous_record_publish_micros: episode.last_publish_micros,
                    previous_record_response_micros: episode.last_response_micros,
                },
            }
        };
        let line = serde_json::to_string(&footer).map_err(io::Error::other)?;
        self.append_and_publish_v4(line, false, 0)?;
        self.open = None;
        Ok(())
    }

    /// Appends `line` and republishes the episode file.
    ///
    /// COMMIT-AFTER-PUBLISH. Every piece of chain state (the running hash,
    /// the accumulated lines, the record ordinal, and the decision
    /// ordinal) is computed into locals and written back to `self.open`
    /// only once `publish_atomically_v4` has returned success. The earlier
    /// shape advanced that state first, so a failed publish left the
    /// writer believing a record it had never published was already on
    /// disk: the caller restores the game session and retries, the SAME
    /// decision is published a second time under later ordinals, and the
    /// chain still verifies because the chain covers whatever bytes were
    /// actually written. A duplicated decision that passes verification is
    /// strictly worse than a missing one, because nothing downstream can
    /// detect it.
    ///
    /// The publish is an atomic replacement, so on failure the previously
    /// published file is still the newest complete file on disk, which is
    /// exactly the state the un-advanced writer describes. Retry is
    /// therefore safe and idempotent, which is why staging is the right
    /// fix here rather than poisoning the runtime: a transient ENOSPC or a
    /// held file handle should cost the panel one retry, not the rest of
    /// the episode.
    fn append_and_publish_v4(
        &mut self,
        line: String,
        is_decision: bool,
        record_decision_micros: u64,
    ) -> io::Result<()> {
        let episode = self
            .open
            .as_ref()
            .ok_or_else(|| io::Error::other("no open search-diagnostics episode"))?;
        if line.contains('\n') || line.contains('\r') {
            return Err(io::Error::other("a JSONL record may not contain a newline"));
        }
        let staged_previous = lower_hex_sha256_v4(record_chain_link_v4(&line));
        let mut bytes = Vec::new();
        for published in &episode.lines {
            bytes.extend_from_slice(published.as_bytes());
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        let path = episode.path.clone();
        // The publication is SYNCHRONOUS: it rewrites, syncs, and
        // reverifies the episode file before the caller can respond to its
        // client, so its cost is part of the protocol latency. Measured
        // here and carried into the next record; see `WallTimeV4`.
        let publish_started = Instant::now();
        publish_atomically_v4(&path, &bytes)?;
        #[cfg(test)]
        if let Some(delay) = self.publish_delay_for_test_v4 {
            std::thread::sleep(delay);
        }
        let publish_micros =
            u64::try_from(publish_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        // Commit. Nothing above this line mutated the writer.
        let episode = self
            .open
            .as_mut()
            .ok_or_else(|| io::Error::other("no open search-diagnostics episode"))?;
        episode.previous_record_sha256 = staged_previous;
        episode.lines.push(line);
        episode.next_record_ordinal += 1;
        episode.last_publish_micros = publish_micros;
        episode.last_record_decision_micros = record_decision_micros;
        // The tail that follows THIS publish has not happened yet, so it
        // reads as zero until the response boundary reports it.
        episode.last_response_micros = 0;
        episode.response_accounting_pending = true;
        if is_decision {
            episode.next_decision_ordinal += 1;
        }
        Ok(())
    }
}

/// Publishes `bytes` at `path` by atomic REPLACEMENT.
///
/// Delegates to `durable_move_publication_v2::replace_file_by_move_v2`,
/// which stages the bytes, syncs them, and then performs a single
/// replacing move: on Windows exactly `MoveFileExW(stage, final,
/// MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`, elsewhere
/// `rename`. It reopens and reverifies the published length and digest
/// before returning, so a short or corrupted publish is an error rather
/// than a silently truncated diagnostics file.
///
/// The destination is NEVER unlinked first. The earlier shape removed the
/// existing file and then renamed, which opened a window in which a reader
/// observes no file at all and a crash loses the last published record
/// outright, defeating the entire point of republishing after every
/// decision. Replacement closes that window: at every instant the path
/// names either the previous complete file or the new one.
fn publish_atomically_v4(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::other("search diagnostics path has no file name"))?
        .to_owned();
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("search diagnostics path has no parent"))?;
    let mut stage_name = file_name.clone();
    stage_name.push(".tmp");
    let stage_path = directory.join(&stage_name);

    // A pre-existing stage file is STALE, always, and is cleared rather
    // than treated as a conflict.
    //
    // `replace_file_by_move_v2` opens the stage with `create_new`, so a
    // leftover stage makes every subsequent attempt fail with a stage
    // collision: one interrupted publish (ENOSPC, a sharing violation, a
    // kill between staging and the move) would otherwise poison the rest
    // of the episode, turning a transient fault into a permanent one. The
    // stage name is by construction never the published artifact and
    // never anything a reader consumes, so nothing can be lost by
    // removing it. Only a failure to remove it is an error, and it is
    // reported as itself rather than as the collision it would later
    // cause.
    clear_stale_stage_file_v4(&stage_path)?;

    let parent = capture_existing_publication_parent_v1(directory)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let expectation = DurableFileExpectationV1::from_bytes(bytes)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let published = replace_file_by_move_v2(&parent, &stage_name, &file_name, bytes, expectation)
        .map_err(|error| io::Error::other(format!("{error:?}")));
    if published.is_err() {
        // Leave no stage behind on the way out either, so the NEXT
        // attempt starts from the same clean state this one did. Best
        // effort: the publish error is the one worth reporting, and the
        // entry-side sweep above is what guarantees recovery even if this
        // removal could not run (a crash, say).
        let _ = fs::remove_file(&stage_path);
    }
    published?;
    Ok(())
}

/// Removes a leftover staging file, tolerating its absence.
///
/// Only a real removal failure is propagated; "it was not there" is the
/// normal case and is success.
fn clear_stale_stage_file_v4(stage_path: &Path) -> io::Result<()> {
    match fs::remove_file(stage_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io::Error::other(format!(
            "a stale search-diagnostics staging file could not be removed: {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_native_search_opponent_v1::KernelNativeSearchActionStatV1;

    fn scratch_directory_v4(tag: u32) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "mtg-kernel-search-diag-{}-{tag}",
            std::process::id()
        ));
        fs::remove_dir_all(&directory).ok();
        fs::create_dir_all(&directory).expect("scratch directory");
        directory
    }

    fn wrapper_identity_v4() -> WrapperIdentityV4 {
        WrapperIdentityV4 {
            core_algorithm_identity: "algorithm/v1".to_owned(),
            authority_kind: "model-guided-searcher-v1".to_owned(),
            authority_schema: "model_guided_searcher_authority/v1".to_owned(),
            node_key_identity: "node-key/v1".to_owned(),
            seed_domain: "seed-domain/v1".to_owned(),
            tier: "t512".to_owned(),
            transition_budget: 512,
            policy_step_depth_cap: 64,
            seed_block_id: 0,
            action_seed_u64_hex: "00000000002f5c49".to_owned(),
            search_authority_digest_sha256: "a".repeat(64),
            checkpoint_lineage_id: "store|loaded_run_sha256=abc".to_owned(),
            net_architecture_identity: "kernel-policy-value-net-8".to_owned(),
            puct_prior_quantization_contract_sha256: "b".repeat(64),
            value_quantization_contract_sha256: "c".repeat(64),
            forward_determinism_build_identity: "d".repeat(64),
            value_head_domain: "calibrated:-1,1".to_owned(),
            checkpoint_manifest_sha256: "e".repeat(64),
            checkpoint_model_parameter_sha256: "f".repeat(64),
            engine_commit: "0".repeat(40),
        }
    }

    fn decision_v1(selected_index: u32, visits: &[u32]) -> ModelGuidedSearchDecisionV1 {
        ModelGuidedSearchDecisionV1 {
            selected_index,
            transitions_used: 512,
            simulations: 40,
            tree_node_count: 33,
            root_action_stats: visits
                .iter()
                .enumerate()
                .map(|(index, &visits)| KernelNativeSearchActionStatV1 {
                    flat_action_index: index as u32,
                    visits,
                    value_sum: i64::from(visits) * 10,
                    mean_value: 10,
                })
                .collect(),
            leaf_census: ModelGuidedSearchLeafCensusV1 {
                natural_terminal_leaves: 5,
                truncated_terminal_leaves: 0,
                newly_expanded_leaves: 30,
                depth_cap_leaves: 5,
                max_simulation_depth: 12,
                summed_simulation_depth: 300,
            },
        }
    }

    fn decision_record_v4(episode_id: u64, wall: WallTimeV4) -> SearchDecisionRecordV4 {
        let decision = decision_v1(1, &[10, 25, 5]);
        SearchDecisionRecordV4 {
            contract: String::new(),
            schema_version: 0,
            record_kind: String::new(),
            record_ordinal: 0,
            previous_record_sha256: String::new(),
            episode_id,
            decision_ordinal: 0,
            step: 7,
            physical_decision_id: 3,
            substep_index: 0,
            acting_player: PlayerSeatV1::P0,
            legal_action_count: 3,
            search_authority_digest_sha256: "a".repeat(64),
            requested_transitions: 512,
            actual_transitions: decision.transitions_used,
            simulations: decision.simulations,
            tree_node_count: decision.tree_node_count,
            leaf_census: decision.leaf_census,
            root_statistics_digest_sha256: lower_hex_sha256_v4(root_statistics_digest_v4(
                &decision,
            )),
            chosen_action_index: decision.selected_index,
            visit_margin: visit_margin_v4(&decision),
            policy_sample_index: 0,
            search_overrode_policy_sample: true,
            stability_halves_enabled: true,
            stability: Some(StabilityV4 {
                half_a_selected_index: 1,
                half_b_selected_index: 1,
                half_transition_budget: 256,
                halves_agree: true,
                halves_agree_with_full_budget: true,
            }),
            protocol_request_kind: ProtocolRequestKindV4::Step,
            search_ceiling_status: CeilingStatusV4::WithinSlo,
            wall_time: wall,
        }
    }

    #[test]
    fn visit_margin_is_the_gap_to_the_runner_up_v4() {
        assert_eq!(visit_margin_v4(&decision_v1(1, &[10, 25, 5])), 15);
        assert_eq!(visit_margin_v4(&decision_v1(0, &[7])), 7);
        // A tie at the top yields a zero margin, not a wrapped one.
        assert_eq!(visit_margin_v4(&decision_v1(0, &[9, 9])), 0);
    }

    #[test]
    fn root_statistics_digest_separates_same_verdict_different_search_v4() {
        let a = decision_v1(1, &[10, 25, 5]);
        let b = decision_v1(1, &[11, 24, 5]);
        assert_eq!(a.selected_index, b.selected_index);
        assert_ne!(root_statistics_digest_v4(&a), root_statistics_digest_v4(&b));
    }

    /// The wire identity moved to v4 with the record shape. A record that
    /// declares v3 is a different, incompatible schema and this writer
    /// must never emit one.
    #[test]
    fn published_records_declare_the_v4_wire_identity_v4() {
        assert_eq!(
            MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V4,
            "mtg-kernel-model-guided-search-outcome-jsonl/v4"
        );
        assert_eq!(MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V4, 4);
        let directory = scratch_directory_v4(13);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(11, 404, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        writer
            .write_decision_v4(decision_record_v4(11, WallTimeV4::default()))
            .expect("decision publishes");
        writer
            .close_episode_v4(EpisodeCloseReasonV4::EpisodeTerminal)
            .expect("footer publishes");
        let text = fs::read_to_string(writer.episode_path_v4(11, 404)).unwrap();
        for line in text.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(
                value["contract"],
                "mtg-kernel-model-guided-search-outcome-jsonl/v4"
            );
            assert_eq!(value["schema_version"], 4);
        }
        // The file name carries the identity too, so a v3 and a v4 episode
        // of the same id could never collide on one path.
        assert!(writer
            .episode_path_v4(11, 404)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("model_guided_search_outcome_v4_episode_"));
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn ceiling_status_classifies_against_the_pre_registered_bounds_v4() {
        assert_eq!(
            CeilingStatusV4::classify_v4(0.0),
            CeilingStatusV4::WithinSlo
        );
        assert_eq!(
            CeilingStatusV4::classify_v4(4.0),
            CeilingStatusV4::WithinSlo
        );
        assert_eq!(
            CeilingStatusV4::classify_v4(4.000_001),
            CeilingStatusV4::SloExceeded
        );
        assert_eq!(
            CeilingStatusV4::classify_v4(19.999),
            CeilingStatusV4::SloExceeded
        );
        assert_eq!(
            CeilingStatusV4::classify_v4(20.0),
            CeilingStatusV4::HardTimeoutExceeded
        );
    }

    #[test]
    fn published_episode_is_chain_valid_after_every_record_v4() {
        let directory = scratch_directory_v4(1);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(9, 0x1234, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        let path = writer.episode_path_v4(9, 0x1234);
        // The header alone is already a complete, chain-valid file: the
        // whole point of publishing per record.
        assert_eq!(verify_episode_chain_v4(&fs::read(&path).unwrap()), Ok(1));
        for expected in 2..=4 {
            writer
                .write_decision_v4(decision_record_v4(9, WallTimeV4::default()))
                .expect("decision publishes");
            assert_eq!(
                verify_episode_chain_v4(&fs::read(&path).unwrap()),
                Ok(expected)
            );
        }
        assert!(!path.with_extension("jsonl.tmp").exists());
        // Ordinals are writer-assigned and monotone, independent of the
        // caller's (here always zero) values.
        let text = fs::read_to_string(&path).unwrap();
        for (index, line) in text.lines().enumerate() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(value["record_ordinal"].as_u64(), Some(index as u64));
            if index > 0 {
                assert_eq!(value["decision_ordinal"].as_u64(), Some(index as u64 - 1));
            }
        }
        fs::remove_dir_all(&directory).ok();
    }

    /// A failed publish must leave the writer exactly where it was, so a
    /// retry republishes the SAME decision at the SAME ordinals instead of
    /// appending a duplicate under later ones.
    ///
    /// The failure is induced by occupying the staging name with a
    /// directory, which no amount of retrying inside the writer can work
    /// around, and which leaves the already-published file untouched: the
    /// precise shape of a transient publish failure.
    #[test]
    fn a_failed_publish_does_not_advance_the_writer_and_retry_publishes_once_v4() {
        let directory = scratch_directory_v4(7);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(3, 77, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        let path = writer.episode_path_v4(3, 77);
        writer
            .write_decision_v4(decision_record_v4(3, WallTimeV4::default()))
            .expect("first decision publishes");
        assert_eq!(verify_episode_chain_v4(&fs::read(&path).unwrap()), Ok(2));
        let published_before = fs::read(&path).unwrap();
        assert_eq!(writer.next_decision_ordinal_v4().unwrap(), 1);

        // Block the staging name so the publish cannot succeed.
        let mut stage = path.clone().into_os_string();
        stage.push(".tmp");
        let stage = PathBuf::from(stage);
        fs::create_dir(&stage).expect("stage name is occupiable");
        assert!(writer
            .write_decision_v4(decision_record_v4(3, WallTimeV4::default()))
            .is_err());

        // Nothing moved: not the file on disk, not the writer's ordinals.
        assert_eq!(fs::read(&path).unwrap(), published_before);
        assert_eq!(
            writer.next_decision_ordinal_v4().unwrap(),
            1,
            "a failed publish must not consume a decision ordinal"
        );

        // A failed CLOSE is retryable for the same reason: the episode is
        // still open and its footer is still owed.
        assert!(writer
            .close_episode_v4(EpisodeCloseReasonV4::EpisodeTerminal)
            .is_err());
        assert!(writer.has_open_episode_v4());

        // Unblock and retry. Exactly one new record appears, at the
        // ordinal the failed attempt would have used.
        fs::remove_dir(&stage).expect("stage name frees");
        writer
            .write_decision_v4(decision_record_v4(3, WallTimeV4::default()))
            .expect("retry publishes");
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(verify_episode_chain_v4(text.as_bytes()), Ok(3));
        let decisions: Vec<u64> = text
            .lines()
            .skip(1)
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["decision_ordinal"]
                    .as_u64()
                    .unwrap()
            })
            .collect();
        assert_eq!(
            decisions,
            vec![0, 1],
            "the retry must not duplicate the decision or skip an ordinal"
        );
        fs::remove_dir_all(&directory).ok();
    }

    /// The publish never unlinks the destination, so the path always names
    /// a complete file. Verified by checking that the file is continuously
    /// present and chain-valid across a whole episode of republishes, and
    /// that the inode/identity is REPLACED rather than the file being
    /// removed and recreated with a gap.
    #[test]
    fn publication_replaces_in_place_and_never_unlinks_the_destination_v4() {
        let directory = scratch_directory_v4(8);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(4, 88, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        let path = writer.episode_path_v4(4, 88);
        for expected in 2..=5 {
            let before = fs::read(&path).expect("the published file exists before the republish");
            assert!(verify_episode_chain_v4(&before).is_ok());
            writer
                .write_decision_v4(decision_record_v4(4, WallTimeV4::default()))
                .expect("decision publishes");
            let after = fs::read(&path).expect("the published file exists after the republish");
            assert_eq!(verify_episode_chain_v4(&after), Ok(expected));
            assert!(
                after.starts_with(&before),
                "a republish must extend the previous file, never rewrite its history"
            );
        }
        // The staging name is not left behind.
        let mut stage = path.clone().into_os_string();
        stage.push(".tmp");
        assert!(!PathBuf::from(stage).exists());
        fs::remove_dir_all(&directory).ok();
    }

    /// A leftover staging file must not poison the episode.
    ///
    /// This is the exact state a publish interrupted between staging and
    /// the move leaves behind (ENOSPC, a sharing violation, a kill).
    /// Because `replace_file_by_move_v2` opens the stage with
    /// `create_new`, that leftover used to make EVERY subsequent publish
    /// fail with a stage collision, turning one transient fault into a
    /// permanently dead episode. A stage file is by construction never the
    /// published artifact, so the right response is to clear it.
    #[test]
    fn a_leftover_stage_file_is_stale_and_does_not_block_publication_v4() {
        let directory = scratch_directory_v4(9);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(5, 99, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        let path = writer.episode_path_v4(5, 99);
        writer
            .write_decision_v4(decision_record_v4(5, WallTimeV4::default()))
            .expect("first decision publishes");

        // Simulate the interrupted publish: a stage file with junk in it.
        let mut stage = path.clone().into_os_string();
        stage.push(".tmp");
        let stage = PathBuf::from(stage);
        fs::write(&stage, b"a half-written staging artifact").expect("stage file writes");
        assert!(stage.exists());

        // The next publish must succeed anyway, and must consume the
        // stale stage rather than colliding with it.
        writer
            .write_decision_v4(decision_record_v4(5, WallTimeV4::default()))
            .expect("a stale stage file must not block publication");
        assert!(
            !stage.exists(),
            "the stage file must not survive a successful publish"
        );
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(verify_episode_chain_v4(text.as_bytes()), Ok(3));
        // The junk never reached the published file.
        assert!(!text.contains("half-written"));
        fs::remove_dir_all(&directory).ok();
    }

    /// A failed publish must leave no staging file behind, so the retry
    /// starts from the same clean state the failed attempt did.
    #[test]
    fn a_failed_publish_leaves_no_stage_file_and_the_retry_succeeds_v4() {
        let directory = scratch_directory_v4(10);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(6, 101, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        let path = writer.episode_path_v4(6, 101);
        let mut stage = path.clone().into_os_string();
        stage.push(".tmp");
        let stage = PathBuf::from(stage);

        // Block the publish by occupying the STAGE name with a directory,
        // which `remove_file` cannot clear and the stage open cannot use.
        fs::create_dir(&stage).expect("stage name is occupiable");
        assert!(writer
            .write_decision_v4(decision_record_v4(6, WallTimeV4::default()))
            .is_err());
        // The blocking directory is still a directory: the cleanup must
        // not have blindly destroyed something it did not create.
        assert!(stage.is_dir());

        // Clear the block; the retry must succeed and leave no stage.
        fs::remove_dir(&stage).expect("stage name frees");
        writer
            .write_decision_v4(decision_record_v4(6, WallTimeV4::default()))
            .expect("the retry must succeed");
        assert!(
            !stage.exists(),
            "no staging file may survive a successful publish"
        );
        assert_eq!(verify_episode_chain_v4(&fs::read(&path).unwrap()), Ok(2));

        // And an ordinary sequence after the recovery still chains.
        writer
            .write_decision_v4(decision_record_v4(6, WallTimeV4::default()))
            .expect("publishing continues");
        assert_eq!(verify_episode_chain_v4(&fs::read(&path).unwrap()), Ok(3));
        assert!(!stage.exists());
        fs::remove_dir_all(&directory).ok();
    }

    /// CODEX P1. `decision_micros` stops before the record's own
    /// publication, but that publication is synchronous and completes
    /// before the client can be answered. A slow publish can therefore
    /// push the real protocol latency across a pre-registered boundary
    /// while the record's own `search_ceiling_status` still says
    /// `within_slo`.
    ///
    /// The publish is made artificially slow so the crossing is real and
    /// not merely arithmetic: the injected delay sits INSIDE the measured
    /// publication window, exactly where a slow disk would.
    #[test]
    fn a_slow_publication_is_charged_to_the_decision_it_publishes_v4() {
        let directory = scratch_directory_v4(11);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(7, 202, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        let path = writer.episode_path_v4(7, 202);

        // A decision comfortably inside the SLO on its own.
        let fast_decision = WallTimeV4 {
            full_search_micros: 3_500_000,
            stability_half_a_micros: 0,
            stability_half_b_micros: 0,
            search_micros: 3_500_000,
            decision_micros: 3_900_000,
            previous_record_publish_micros: 0,
            previous_record_response_micros: 0,
        };
        writer.set_publish_delay_for_test_v4(std::time::Duration::from_millis(250));
        writer
            .write_decision_v4(decision_record_v4(7, fast_decision))
            .expect("slow decision publishes");
        // A second record, whose only job here is to observe the first
        // one's publication.
        writer
            .write_decision_v4(decision_record_v4(7, WallTimeV4::default()))
            .expect("second decision publishes");

        let bytes = fs::read(&path).unwrap();
        let ceilings = episode_decision_ceilings_v4(&bytes).expect("chain verifies");
        assert_eq!(ceilings.len(), 2);

        let first = ceilings[0];
        assert_eq!(first.decision_ordinal, 0);
        assert_eq!(first.decision_micros, 3_900_000);
        assert_eq!(first.search_micros, 3_500_000);
        let publish = first
            .publish_micros
            .expect("the successor record observed this record's publication");
        assert!(
            publish >= 250_000,
            "the injected 250 ms delay must be charged to the publish: {publish} us"
        );
        // The record's own field, over the search alone, is within SLO.
        assert_eq!(first.search_ceiling_status, CeilingStatusV4::WithinSlo);
        // The protocol verdict, which includes the publication, is not:
        // 3.9 s + 0.25 s crosses the 4.0 s SLO. This is precisely the
        // under-report the record alone would have produced.
        assert_eq!(
            first.protocol_ceiling_status,
            Some(CeilingStatusV4::SloExceeded),
            "a synchronous publish that crosses the SLO must be visible"
        );

        // Before the footer, the LAST record's publication has no
        // successor to observe it, and that gap is reported honestly.
        let last = ceilings[1];
        assert_eq!(last.publish_micros, None);
        assert_eq!(last.protocol_ceiling_status, None);

        // CODEX P1, the fix: closing the episode publishes a footer whose
        // `previous_record_publish_micros` IS that last publication, so no
        // decision is left unclassified.
        writer
            .close_episode_v4(EpisodeCloseReasonV4::EpisodeTerminal)
            .expect("footer publishes");
        let closed = fs::read(&path).unwrap();
        let ceilings = episode_decision_ceilings_v4(&closed).expect("chain verifies");
        assert_eq!(ceilings.len(), 2);
        assert!(
            ceilings
                .iter()
                .all(|ceiling| ceiling.protocol_ceiling_status.is_some()),
            "a closed episode classifies every decision it recorded"
        );
        let last = ceilings[1];
        assert!(
            last.publish_micros.expect("the footer observed it") >= 250_000,
            "the footer carries the last decision's real publication"
        );
        fs::remove_dir_all(&directory).ok();
    }

    /// The chain-level rule is arithmetic over two adjacent records, so it
    /// is worth pinning directly: a decision's latency is its OWN
    /// `decision_micros` plus its SUCCESSOR's reported publish time, never
    /// its own `previous_record_publish_micros` (which belongs to the
    /// record before it).
    #[test]
    fn decision_ceilings_attribute_each_publish_to_the_record_it_published_v4() {
        let directory = scratch_directory_v4(12);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(8, 303, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        for decision_micros in [1_000_000u64, 2_000_000, 3_000_000] {
            writer
                .write_decision_v4(decision_record_v4(
                    8,
                    WallTimeV4 {
                        decision_micros,
                        ..WallTimeV4::default()
                    },
                ))
                .expect("decision publishes");
        }
        writer
            .close_episode_v4(EpisodeCloseReasonV4::ProcessExit)
            .expect("footer publishes");
        let bytes = fs::read(writer.episode_path_v4(8, 303)).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        let reported: Vec<u64> = text
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["wall_time"]
                    ["previous_record_publish_micros"]
                    .as_u64()
                    .unwrap_or(0)
            })
            .collect();
        let ceilings = episode_decision_ceilings_v4(&bytes).expect("chain verifies");
        assert_eq!(ceilings.len(), 3);
        assert_eq!(
            ceilings
                .iter()
                .map(|c| c.decision_micros)
                .collect::<Vec<_>>(),
            vec![1_000_000, 2_000_000, 3_000_000]
        );
        // Decision n's publish time is what record n+1 reported, and the
        // footer is record n+1 for the last one.
        assert_eq!(ceilings[0].publish_micros, Some(reported[2]));
        assert_eq!(ceilings[1].publish_micros, Some(reported[3]));
        assert_eq!(ceilings[2].publish_micros, Some(reported[4]));
        assert_eq!(
            ceilings[2].protocol_micros,
            Some(3_000_000 + reported[4]),
            "the final decision's protocol latency comes from the footer"
        );
        // A tampered file yields no latency report at all.
        let tampered = text.replacen(
            "\"decision_micros\":1000000",
            "\"decision_micros\":1000001",
            1,
        );
        assert!(episode_decision_ceilings_v4(tampered.as_bytes()).is_err());
        fs::remove_dir_all(&directory).ok();
    }

    /// CODEX P1, round 2. The record is on disk before the response is
    /// serialized, written and flushed and before any later export runs,
    /// so charging only the search and the publication still lets a slow
    /// output path push the request the client actually waited on past a
    /// pre-registered ceiling while the classification reports it inside.
    ///
    /// The tail after the publish is measured at the outer boundary,
    /// carried by the successor exactly as the publish time is, and added
    /// by the classifier. The three phases reconstruct the whole request
    /// by construction, which is what this pins.
    #[test]
    fn a_slow_response_tail_is_charged_to_the_decision_it_answered_v4() {
        let directory = scratch_directory_v4(16);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(14, 707, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        // A decision comfortably inside the SLO on everything the record
        // can see about itself.
        let near_slo = WallTimeV4 {
            full_search_micros: 3_500_000,
            search_micros: 3_500_000,
            decision_micros: 3_900_000,
            ..WallTimeV4::default()
        };
        writer
            .write_decision_v4(decision_record_v4(14, near_slo))
            .expect("decision publishes");
        // The client's wait for that decision ended 4.1 s after its
        // request arrived: everything past 3.9 s happened after the record
        // was already published.
        writer.note_request_completed_v4(4_100_000);
        // A second boundary for the same publish changes nothing: the tail
        // was already attributed, and a later request that published no
        // record must not overwrite it.
        writer.note_request_completed_v4(50);
        writer
            .close_episode_v4(EpisodeCloseReasonV4::EpisodeTerminal)
            .expect("footer publishes");

        let bytes = fs::read(writer.episode_path_v4(14, 707)).unwrap();
        let ceilings = episode_decision_ceilings_v4(&bytes).expect("chain verifies");
        assert_eq!(ceilings.len(), 1);
        let only = ceilings[0];
        assert_eq!(only.decision_micros, 3_900_000);
        // The 200 ms past the record's own window is accounted for, wherever
        // it fell: the tail carries it, unless a pathologically slow publish
        // swallowed it, in which case the publish carries it instead.
        // Either way it is charged to this decision and neither is dropped.
        let tail = only.response_micros.expect("the footer reports the tail");
        let publish = only.publish_micros.expect("the footer reports the publish");
        assert!(
            tail + publish >= 200_000,
            "the interval after the record was built must be charged: tail {tail} publish {publish}"
        );
        assert!(
            only.protocol_micros.unwrap() >= 4_100_000,
            "the three phases must reconstruct the client's whole wait: {:?}",
            only.protocol_micros
        );
        // The record's own field, over the search alone, is within SLO.
        // The protocol verdict is not, and only the chain-level read can
        // say so.
        assert_eq!(only.search_ceiling_status, CeilingStatusV4::WithinSlo);
        assert_eq!(
            only.protocol_ceiling_status,
            Some(CeilingStatusV4::SloExceeded),
            "a slow response path that crosses the SLO must be visible"
        );
        fs::remove_dir_all(&directory).ok();
    }

    /// CODEX P1. The footer is what makes an episode COMPLETE, so the
    /// verifier has to enforce that nothing follows it and that it
    /// commits to the bytes it closes over. Otherwise "the file ends in a
    /// footer" would not mean "this is the whole episode".
    #[test]
    fn the_footer_is_terminal_and_commits_to_the_episode_v4() {
        let directory = scratch_directory_v4(14);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(12, 505, PlayerSeatV1::P1, wrapper_identity_v4())
            .expect("header publishes");
        writer
            .write_decision_v4(decision_record_v4(12, WallTimeV4::default()))
            .expect("decision publishes");
        writer
            .close_episode_v4(EpisodeCloseReasonV4::EpisodeReplaced)
            .expect("footer publishes");
        let path = writer.episode_path_v4(12, 505);
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(verify_episode_chain_v4(text.as_bytes()), Ok(3));

        let footer: EpisodeFooterRecordV4 =
            serde_json::from_str(text.lines().next_back().unwrap()).unwrap();
        assert_eq!(footer.record_kind, EPISODE_FOOTER_RECORD_KIND_V4);
        assert_eq!(footer.close_reason, EpisodeCloseReasonV4::EpisodeReplaced);
        assert_eq!(footer.decision_record_count, 1);
        // The footer's digest covers exactly the bytes before its line.
        let footer_line_len = text.lines().next_back().unwrap().len() + 1;
        let content = &text.as_bytes()[..text.len() - footer_line_len];
        assert_eq!(
            footer.episode_content_sha256,
            lower_hex_sha256_v4(episode_content_digest_v4(content))
        );

        // The episode is CLOSED: no further record may be written to it.
        assert!(writer
            .write_decision_v4(decision_record_v4(12, WallTimeV4::default()))
            .is_err());
        assert!(writer
            .close_episode_v4(EpisodeCloseReasonV4::ProcessExit)
            .is_err());
        assert!(!writer.has_open_episode_v4());

        // A decision APPENDED after the footer is rejected even though its
        // chain link is perfectly valid: it is a forged continuation of a
        // closed episode.
        let mut forged = decision_record_v4(12, WallTimeV4::default());
        forged.contract = MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V4.to_owned();
        forged.schema_version = MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V4;
        forged.record_kind = SEARCH_DECISION_RECORD_KIND_V4.to_owned();
        forged.record_ordinal = 3;
        forged.decision_ordinal = 1;
        forged.previous_record_sha256 =
            lower_hex_sha256_v4(record_chain_link_v4(text.lines().next_back().unwrap()));
        let appended = format!("{text}{}\n", serde_json::to_string(&forged).unwrap());
        let error = verify_episode_chain_v4(appended.as_bytes()).expect_err("footer is terminal");
        assert!(error.contains("follows the episode footer"), "{error}");
        fs::remove_dir_all(&directory).ok();
    }

    /// Structural rejections the footer rule depends on: an unknown record
    /// kind, a headerless file, and a footer whose episode digest does not
    /// match the bytes it closes over.
    #[test]
    fn the_verifier_rejects_malformed_episode_structure_v4() {
        let directory = scratch_directory_v4(15);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(13, 606, PlayerSeatV1::P0, wrapper_identity_v4())
            .expect("header publishes");
        writer
            .write_decision_v4(decision_record_v4(13, WallTimeV4::default()))
            .expect("decision publishes");
        writer
            .close_episode_v4(EpisodeCloseReasonV4::EpisodeTerminal)
            .expect("footer publishes");
        let text = fs::read_to_string(writer.episode_path_v4(13, 606)).unwrap();
        assert_eq!(verify_episode_chain_v4(text.as_bytes()), Ok(3));

        // A file that does not start with a header.
        let headerless: String = text
            .lines()
            .skip(1)
            .map(|line| format!("{line}\n"))
            .collect();
        assert!(verify_episode_chain_v4(headerless.as_bytes()).is_err());

        // An unknown record kind is rejected, not skipped: this reader
        // must not pretend to understand a forward-version record.
        let unknown = text.replacen("\"search_decision\"", "\"search_decision_v9\"", 1);
        let error = verify_episode_chain_v4(unknown.as_bytes())
            .expect_err("an unknown kind cannot be skipped");
        assert!(error.contains("unknown record_kind"), "{error}");

        // A footer whose decision count lies is rejected even though the
        // chain links still verify.
        let miscounted = text.replacen(
            "\"decision_record_count\":1",
            "\"decision_record_count\":2",
            1,
        );
        assert!(verify_episode_chain_v4(miscounted.as_bytes()).is_err());
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_tampered_record_breaks_the_chain_v4() {
        let directory = scratch_directory_v4(2);
        let mut writer = ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v4(1, 5, PlayerSeatV1::P1, wrapper_identity_v4())
            .unwrap();
        writer
            .write_decision_v4(decision_record_v4(1, WallTimeV4::default()))
            .unwrap();
        writer
            .write_decision_v4(decision_record_v4(1, WallTimeV4::default()))
            .unwrap();
        let bytes = fs::read(writer.episode_path_v4(1, 5)).unwrap();
        assert_eq!(verify_episode_chain_v4(&bytes), Ok(3));
        // Editing a decision's own content invalidates the NEXT record's
        // link, which is exactly what chaining is for.
        let tampered = String::from_utf8(bytes)
            .unwrap()
            .replace("\"chosen_action_index\":1", "\"chosen_action_index\":2");
        assert!(verify_episode_chain_v4(tampered.as_bytes()).is_err());
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn only_wall_time_differs_between_two_identical_record_sets_v4() {
        let fast = decision_record_v4(2, WallTimeV4::default());
        let slow = decision_record_v4(
            2,
            WallTimeV4 {
                full_search_micros: 9_000_000,
                stability_half_a_micros: 1,
                stability_half_b_micros: 2,
                search_micros: 9_000_003,
                decision_micros: 9_000_010,
                previous_record_publish_micros: 4,
                previous_record_response_micros: 5,
            },
        );
        assert_ne!(fast, slow);
        let mut normalized = slow;
        normalized.wall_time = WallTimeV4::default();
        assert_eq!(fast, normalized);
    }

    #[test]
    fn writer_rejects_a_missing_directory_and_a_foreign_episode_v4() {
        let missing = std::env::temp_dir().join("mtg-kernel-search-diag-does-not-exist-v4");
        fs::remove_dir_all(&missing).ok();
        assert!(ModelGuidedSearchOutcomeWriterV4::open_directory_v4(missing).is_err());

        let directory = scratch_directory_v4(3);
        let mut writer =
            ModelGuidedSearchOutcomeWriterV4::open_directory_v4(directory.clone()).unwrap();
        // A decision before any episode is open fails closed.
        assert!(writer
            .write_decision_v4(decision_record_v4(4, WallTimeV4::default()))
            .is_err());
        assert!(writer.next_decision_ordinal_v4().is_err());
        // So does a close.
        assert!(writer
            .close_episode_v4(EpisodeCloseReasonV4::ProcessExit)
            .is_err());
        writer
            .begin_episode_v4(4, 1, PlayerSeatV1::P0, wrapper_identity_v4())
            .unwrap();
        assert_eq!(writer.open_episode_id_v4(), Some(4));
        assert_eq!(writer.next_decision_ordinal_v4().unwrap(), 0);
        // A decision belonging to a different episode fails closed rather
        // than being chained into this one.
        assert!(writer
            .write_decision_v4(decision_record_v4(5, WallTimeV4::default()))
            .is_err());
        // Opening a second episode over an open one fails closed too: the
        // first one's footer is still owed.
        assert!(writer
            .begin_episode_v4(6, 2, PlayerSeatV1::P0, wrapper_identity_v4())
            .is_err());
        assert_eq!(writer.open_episode_id_v4(), Some(4));
        assert_eq!(writer.directory_v4(), directory.as_path());
        fs::remove_dir_all(&directory).ok();
    }
}
