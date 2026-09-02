//! Outcome schema V3: per-decision diagnostics for the model-guided
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
//! # Three properties this writer is built around
//!
//! **Wall time is diagnostic and never affects the chosen action.** The
//! owner law is unconditional, so this module never returns a timing to
//! the selector, never compares one against a threshold in order to decide
//! anything, and records the SLO/hard-timeout comparison as a STATUS
//! STRING only. [`CeilingStatusV3`] is produced after the search has
//! already finished and its result is already fixed; there is no code path
//! by which a slow decision selects differently from a fast one. The
//! bit-identical replay test in the scorer proves the consequence: two
//! runs produce byte-identical diagnostics apart from the wall-time
//! fields, which are grouped into [`WallTimeV3`] precisely so a test (or
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
//! **Commit-after-publish.** No chain state advances until the publish has
//! succeeded, so a failed publish is retryable and a retry republishes the
//! same decision at the same ordinals. See `append_and_publish_v3`.
//!
//! **Hash chaining across decisions within an episode.** Each record
//! carries `previous_record_sha256`, the SHA-256 of the previous record's
//! serialized line INCLUDING the newline the file actually contains; the
//! header carries sixty-four zeros. Chaining by predecessor rather than by
//! a self-referential `record_sha256` avoids the usual "hash the record
//! with its own hash field blanked" contortion, which is easy to implement
//! subtly differently in the verifier than in the writer.
//! [`verify_episode_chain_v3`] is the verifier, and the writer's own tests
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

pub const MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V3: &str =
    "mtg-kernel-model-guided-search-outcome-jsonl/v3";
pub const MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V3: u32 = 3;

/// The genesis chain link: sixty-four zeros, never a real SHA-256 of any
/// record.
pub const MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V3: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Pre-registered per-decision wall-time service level, in seconds (sketch
/// Section 4: "p99 per-decision wall time < 4.0 s on the panel host").
/// Recorded, never enforced.
pub const MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V3: f64 = 4.0;

/// Pre-registered hard protocol timeout, in seconds (sketch Section 4:
/// "Hard protocol timeout: 20.0 s"). Also recorded, never enforced HERE:
/// the sketch makes a timeout inside a formal game "a product failure of
/// that panel", which is a panel-protocol consequence, not something a
/// diagnostics writer may act on. Aborting a search on a clock would make
/// the chosen action a function of wall time, which the owner law forbids
/// outright.
pub const MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V3: f64 = 20.0;

/// Where one decision's elapsed time landed against the two pre-registered
/// ceilings. A recorded observation with no control-flow consequence
/// anywhere in this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CeilingStatusV3 {
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

impl CeilingStatusV3 {
    pub fn classify_v3(elapsed_seconds: f64) -> Self {
        if elapsed_seconds >= MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V3 {
            Self::HardTimeoutExceeded
        } else if elapsed_seconds > MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V3 {
            Self::SloExceeded
        } else {
            Self::WithinSlo
        }
    }
}

/// Every wall-time field in the schema, grouped into one struct so a
/// replay test can excise timing in a single move. Nothing outside this
/// struct varies between two runs of the same decision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WallTimeV3 {
    pub full_search_micros: u64,
    pub stability_half_a_micros: u64,
    pub stability_half_b_micros: u64,
    pub total_micros: u64,
}

/// Wrapper identity, carried once per episode in the header record (sketch
/// Section 5: "wrapper identity (core identity, tier, digests, checkpoint
/// manifest SHA-256) in the panel record").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrapperIdentityV3 {
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
pub struct StabilityV3 {
    pub half_a_selected_index: u32,
    pub half_b_selected_index: u32,
    pub half_transition_budget: u32,
    /// The two halves agreed with each other.
    pub halves_agree: bool,
    /// Both halves agreed with the full-budget chosen action.
    pub halves_agree_with_full_budget: bool,
}

/// One episode-opening record, then one record per searched decision. A
/// serde internally-tagged enum is deliberately avoided in favor of an
/// explicit `record_kind` field so the JSON key order stays fully under
/// these structs' own control.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeHeaderRecordV3 {
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
    pub wrapper_identity: WrapperIdentityV3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchDecisionRecordV3 {
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
    pub stability: StabilityV3,
    pub ceiling_status: CeilingStatusV3,
    /// DIAGNOSTIC ONLY. Excluded from every determinism comparison; see
    /// the module docs.
    pub wall_time: WallTimeV3,
}

/// SHA-256 over the root action statistics, in flat-action-index order.
/// Committing to visits, value sums, and means (rather than to the chosen
/// index alone) is what makes this a fingerprint of the SEARCH, not just
/// of its verdict: two runs that pick the same action for different
/// reasons are distinguishable.
pub fn root_statistics_digest_v3(decision: &ModelGuidedSearchDecisionV1) -> [u8; 32] {
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
pub fn visit_margin_v3(decision: &ModelGuidedSearchDecisionV1) -> u32 {
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

pub fn lower_hex_sha256_v3(digest: [u8; 32]) -> String {
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
pub fn record_chain_link_v3(line: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(line.as_bytes());
    hasher.update(b"\n");
    hasher.finalize().into()
}

/// Re-derives the chain over an already-published episode file and returns
/// the record count, or an error naming the first broken link. Used by
/// this module's tests and available to any auditor of a published file.
pub fn verify_episode_chain_v3(bytes: &[u8]) -> Result<usize, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "episode file is not UTF-8".to_owned())?;
    if !text.ends_with('\n') || text.contains('\r') {
        return Err("episode file must be LF-terminated JSONL".to_owned());
    }
    let mut expected_previous = MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V3.to_owned();
    let mut count = 0usize;
    for (index, line) in text.lines().enumerate() {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|_| format!("record {index} is not JSON"))?;
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
        expected_previous = lower_hex_sha256_v3(record_chain_link_v3(line));
        count += 1;
    }
    Ok(count)
}

struct OpenEpisodeV3 {
    episode_id: u64,
    path: PathBuf,
    lines: Vec<String>,
    previous_record_sha256: String,
    next_record_ordinal: u64,
    next_decision_ordinal: u64,
}

/// Per-episode JSONL diagnostics writer: one file per episode in the
/// configured directory, republished atomically after every record.
pub struct ModelGuidedSearchOutcomeWriterV3 {
    directory: PathBuf,
    open: Option<OpenEpisodeV3>,
}

impl ModelGuidedSearchOutcomeWriterV3 {
    /// Opens the writer over an EXISTING, writable directory. The
    /// directory is deliberately not created: silently minting a deep path
    /// is how a typo'd diagnostics flag ends up writing a panel's
    /// diagnostics somewhere nobody looks.
    pub fn open_directory_v3(directory: PathBuf) -> io::Result<Self> {
        if !fs::metadata(&directory)?.is_dir() {
            return Err(io::Error::other(
                "search diagnostics path is not a directory",
            ));
        }
        Ok(Self {
            directory,
            open: None,
        })
    }

    pub fn directory_v3(&self) -> &Path {
        &self.directory
    }

    /// Path this writer uses for one episode. A pure function of (episode
    /// id, base seed), so a replay writes the same file name.
    pub fn episode_path_v3(&self, episode_id: u64, base_seed: u64) -> PathBuf {
        self.directory.join(format!(
            "model_guided_search_outcome_v3_episode_{episode_id:020}_seed_{base_seed:016x}.jsonl"
        ))
    }

    pub fn open_episode_id_v3(&self) -> Option<u64> {
        self.open.as_ref().map(|episode| episode.episode_id)
    }

    /// Opens a new episode, discarding in-memory state from any previous
    /// one (whose file is already published, since publication happens per
    /// record). Publishes the header immediately, so an episode that
    /// searches no decision at all still leaves an auditable file
    /// recording which wrapper was configured.
    pub fn begin_episode_v3(
        &mut self,
        episode_id: u64,
        base_seed: u64,
        candidate_seat: PlayerSeatV1,
        wrapper_identity: WrapperIdentityV3,
    ) -> io::Result<()> {
        let path = self.episode_path_v3(episode_id, base_seed);
        let header = EpisodeHeaderRecordV3 {
            contract: MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V3.to_owned(),
            schema_version: MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V3,
            record_kind: "episode_header".to_owned(),
            record_ordinal: 0,
            previous_record_sha256: MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V3.to_owned(),
            episode_id,
            base_seed_u64_hex: format!("{base_seed:016x}"),
            candidate_seat,
            decision_slo_seconds: MODEL_GUIDED_SEARCH_DECISION_SLO_SECONDS_V3,
            decision_hard_timeout_seconds: MODEL_GUIDED_SEARCH_DECISION_HARD_TIMEOUT_SECONDS_V3,
            wrapper_identity,
        };
        let line = serde_json::to_string(&header).map_err(io::Error::other)?;
        self.open = Some(OpenEpisodeV3 {
            episode_id,
            path,
            lines: Vec::new(),
            previous_record_sha256: MODEL_GUIDED_SEARCH_OUTCOME_CHAIN_GENESIS_V3.to_owned(),
            next_record_ordinal: 0,
            next_decision_ordinal: 0,
        });
        self.append_and_publish_v3(line, false)
    }

    /// The next decision ordinal this writer will assign, so the selector
    /// can carry it in an error report without guessing.
    pub fn next_decision_ordinal_v3(&self) -> io::Result<u64> {
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
    pub fn write_decision_v3(&mut self, mut record: SearchDecisionRecordV3) -> io::Result<()> {
        let (ordinal, decision_ordinal, previous) = {
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
            )
        };
        record.contract = MODEL_GUIDED_SEARCH_OUTCOME_CONTRACT_V3.to_owned();
        record.schema_version = MODEL_GUIDED_SEARCH_OUTCOME_SCHEMA_VERSION_V3;
        record.record_kind = "search_decision".to_owned();
        record.record_ordinal = ordinal;
        record.decision_ordinal = decision_ordinal;
        record.previous_record_sha256 = previous;
        let line = serde_json::to_string(&record).map_err(io::Error::other)?;
        self.append_and_publish_v3(line, true)
    }

    /// Appends `line` and republishes the episode file.
    ///
    /// COMMIT-AFTER-PUBLISH. Every piece of chain state (the running hash,
    /// the accumulated lines, the record ordinal, and the decision
    /// ordinal) is computed into locals and written back to `self.open`
    /// only once `publish_atomically_v3` has returned success. The earlier
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
    fn append_and_publish_v3(&mut self, line: String, is_decision: bool) -> io::Result<()> {
        let episode = self
            .open
            .as_ref()
            .ok_or_else(|| io::Error::other("no open search-diagnostics episode"))?;
        if line.contains('\n') || line.contains('\r') {
            return Err(io::Error::other("a JSONL record may not contain a newline"));
        }
        let staged_previous = lower_hex_sha256_v3(record_chain_link_v3(&line));
        let mut bytes = Vec::new();
        for published in &episode.lines {
            bytes.extend_from_slice(published.as_bytes());
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        let path = episode.path.clone();
        publish_atomically_v3(&path, &bytes)?;
        // Commit. Nothing above this line mutated the writer.
        let episode = self
            .open
            .as_mut()
            .ok_or_else(|| io::Error::other("no open search-diagnostics episode"))?;
        episode.previous_record_sha256 = staged_previous;
        episode.lines.push(line);
        episode.next_record_ordinal += 1;
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
fn publish_atomically_v3(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::other("search diagnostics path has no file name"))?
        .to_owned();
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("search diagnostics path has no parent"))?;
    let mut stage_name = file_name.clone();
    stage_name.push(".tmp");
    let parent = capture_existing_publication_parent_v1(directory)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let expectation = DurableFileExpectationV1::from_bytes(bytes)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    replace_file_by_move_v2(&parent, &stage_name, &file_name, bytes, expectation)
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel_native_search_opponent_v1::KernelNativeSearchActionStatV1;

    fn scratch_directory_v3(tag: u32) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "mtg-kernel-search-diag-{}-{tag}",
            std::process::id()
        ));
        fs::remove_dir_all(&directory).ok();
        fs::create_dir_all(&directory).expect("scratch directory");
        directory
    }

    fn wrapper_identity_v3() -> WrapperIdentityV3 {
        WrapperIdentityV3 {
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

    fn decision_record_v3(episode_id: u64, wall: WallTimeV3) -> SearchDecisionRecordV3 {
        let decision = decision_v1(1, &[10, 25, 5]);
        SearchDecisionRecordV3 {
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
            root_statistics_digest_sha256: lower_hex_sha256_v3(root_statistics_digest_v3(
                &decision,
            )),
            chosen_action_index: decision.selected_index,
            visit_margin: visit_margin_v3(&decision),
            policy_sample_index: 0,
            search_overrode_policy_sample: true,
            stability: StabilityV3 {
                half_a_selected_index: 1,
                half_b_selected_index: 1,
                half_transition_budget: 256,
                halves_agree: true,
                halves_agree_with_full_budget: true,
            },
            ceiling_status: CeilingStatusV3::WithinSlo,
            wall_time: wall,
        }
    }

    #[test]
    fn visit_margin_is_the_gap_to_the_runner_up_v3() {
        assert_eq!(visit_margin_v3(&decision_v1(1, &[10, 25, 5])), 15);
        assert_eq!(visit_margin_v3(&decision_v1(0, &[7])), 7);
        // A tie at the top yields a zero margin, not a wrapped one.
        assert_eq!(visit_margin_v3(&decision_v1(0, &[9, 9])), 0);
    }

    #[test]
    fn root_statistics_digest_separates_same_verdict_different_search_v3() {
        let a = decision_v1(1, &[10, 25, 5]);
        let b = decision_v1(1, &[11, 24, 5]);
        assert_eq!(a.selected_index, b.selected_index);
        assert_ne!(root_statistics_digest_v3(&a), root_statistics_digest_v3(&b));
    }

    #[test]
    fn ceiling_status_classifies_against_the_pre_registered_bounds_v3() {
        assert_eq!(
            CeilingStatusV3::classify_v3(0.0),
            CeilingStatusV3::WithinSlo
        );
        assert_eq!(
            CeilingStatusV3::classify_v3(4.0),
            CeilingStatusV3::WithinSlo
        );
        assert_eq!(
            CeilingStatusV3::classify_v3(4.000_001),
            CeilingStatusV3::SloExceeded
        );
        assert_eq!(
            CeilingStatusV3::classify_v3(19.999),
            CeilingStatusV3::SloExceeded
        );
        assert_eq!(
            CeilingStatusV3::classify_v3(20.0),
            CeilingStatusV3::HardTimeoutExceeded
        );
    }

    #[test]
    fn published_episode_is_chain_valid_after_every_record_v3() {
        let directory = scratch_directory_v3(1);
        let mut writer = ModelGuidedSearchOutcomeWriterV3::open_directory_v3(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v3(9, 0x1234, PlayerSeatV1::P0, wrapper_identity_v3())
            .expect("header publishes");
        let path = writer.episode_path_v3(9, 0x1234);
        // The header alone is already a complete, chain-valid file: the
        // whole point of publishing per record.
        assert_eq!(verify_episode_chain_v3(&fs::read(&path).unwrap()), Ok(1));
        for expected in 2..=4 {
            writer
                .write_decision_v3(decision_record_v3(9, WallTimeV3::default()))
                .expect("decision publishes");
            assert_eq!(
                verify_episode_chain_v3(&fs::read(&path).unwrap()),
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
    fn a_failed_publish_does_not_advance_the_writer_and_retry_publishes_once_v3() {
        let directory = scratch_directory_v3(7);
        let mut writer = ModelGuidedSearchOutcomeWriterV3::open_directory_v3(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v3(3, 77, PlayerSeatV1::P0, wrapper_identity_v3())
            .expect("header publishes");
        let path = writer.episode_path_v3(3, 77);
        writer
            .write_decision_v3(decision_record_v3(3, WallTimeV3::default()))
            .expect("first decision publishes");
        assert_eq!(verify_episode_chain_v3(&fs::read(&path).unwrap()), Ok(2));
        let published_before = fs::read(&path).unwrap();
        assert_eq!(writer.next_decision_ordinal_v3().unwrap(), 1);

        // Block the staging name so the publish cannot succeed.
        let mut stage = path.clone().into_os_string();
        stage.push(".tmp");
        let stage = PathBuf::from(stage);
        fs::create_dir(&stage).expect("stage name is occupiable");
        assert!(writer
            .write_decision_v3(decision_record_v3(3, WallTimeV3::default()))
            .is_err());

        // Nothing moved: not the file on disk, not the writer's ordinals.
        assert_eq!(fs::read(&path).unwrap(), published_before);
        assert_eq!(
            writer.next_decision_ordinal_v3().unwrap(),
            1,
            "a failed publish must not consume a decision ordinal"
        );

        // Unblock and retry. Exactly one new record appears, at the
        // ordinal the failed attempt would have used.
        fs::remove_dir(&stage).expect("stage name frees");
        writer
            .write_decision_v3(decision_record_v3(3, WallTimeV3::default()))
            .expect("retry publishes");
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(verify_episode_chain_v3(text.as_bytes()), Ok(3));
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
    fn publication_replaces_in_place_and_never_unlinks_the_destination_v3() {
        let directory = scratch_directory_v3(8);
        let mut writer = ModelGuidedSearchOutcomeWriterV3::open_directory_v3(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v3(4, 88, PlayerSeatV1::P0, wrapper_identity_v3())
            .expect("header publishes");
        let path = writer.episode_path_v3(4, 88);
        for expected in 2..=5 {
            let before = fs::read(&path).expect("the published file exists before the republish");
            assert!(verify_episode_chain_v3(&before).is_ok());
            writer
                .write_decision_v3(decision_record_v3(4, WallTimeV3::default()))
                .expect("decision publishes");
            let after = fs::read(&path).expect("the published file exists after the republish");
            assert_eq!(verify_episode_chain_v3(&after), Ok(expected));
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

    #[test]
    fn a_tampered_record_breaks_the_chain_v3() {
        let directory = scratch_directory_v3(2);
        let mut writer = ModelGuidedSearchOutcomeWriterV3::open_directory_v3(directory.clone())
            .expect("directory opens");
        writer
            .begin_episode_v3(1, 5, PlayerSeatV1::P1, wrapper_identity_v3())
            .unwrap();
        writer
            .write_decision_v3(decision_record_v3(1, WallTimeV3::default()))
            .unwrap();
        writer
            .write_decision_v3(decision_record_v3(1, WallTimeV3::default()))
            .unwrap();
        let bytes = fs::read(writer.episode_path_v3(1, 5)).unwrap();
        assert_eq!(verify_episode_chain_v3(&bytes), Ok(3));
        // Editing a decision's own content invalidates the NEXT record's
        // link, which is exactly what chaining is for.
        let tampered = String::from_utf8(bytes)
            .unwrap()
            .replace("\"chosen_action_index\":1", "\"chosen_action_index\":2");
        assert!(verify_episode_chain_v3(tampered.as_bytes()).is_err());
        fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn only_wall_time_differs_between_two_identical_record_sets_v3() {
        let fast = decision_record_v3(2, WallTimeV3::default());
        let slow = decision_record_v3(
            2,
            WallTimeV3 {
                full_search_micros: 9_000_000,
                stability_half_a_micros: 1,
                stability_half_b_micros: 2,
                total_micros: 9_000_003,
            },
        );
        assert_ne!(fast, slow);
        let mut normalized = slow;
        normalized.wall_time = WallTimeV3::default();
        assert_eq!(fast, normalized);
    }

    #[test]
    fn writer_rejects_a_missing_directory_and_a_foreign_episode_v3() {
        let missing = std::env::temp_dir().join("mtg-kernel-search-diag-does-not-exist-v3");
        fs::remove_dir_all(&missing).ok();
        assert!(ModelGuidedSearchOutcomeWriterV3::open_directory_v3(missing).is_err());

        let directory = scratch_directory_v3(3);
        let mut writer =
            ModelGuidedSearchOutcomeWriterV3::open_directory_v3(directory.clone()).unwrap();
        // A decision before any episode is open fails closed.
        assert!(writer
            .write_decision_v3(decision_record_v3(4, WallTimeV3::default()))
            .is_err());
        assert!(writer.next_decision_ordinal_v3().is_err());
        writer
            .begin_episode_v3(4, 1, PlayerSeatV1::P0, wrapper_identity_v3())
            .unwrap();
        assert_eq!(writer.open_episode_id_v3(), Some(4));
        assert_eq!(writer.next_decision_ordinal_v3().unwrap(), 0);
        // A decision belonging to a different episode fails closed rather
        // than being chained into this one.
        assert!(writer
            .write_decision_v3(decision_record_v3(5, WallTimeV3::default()))
            .is_err());
        assert_eq!(writer.directory_v3(), directory.as_path());
        fs::remove_dir_all(&directory).ok();
    }
}
