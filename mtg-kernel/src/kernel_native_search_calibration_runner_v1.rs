//! Calibration harness for the kernel-native search opponent
//! (`KERNEL-NATIVE-SEARCH-OPPONENT-V1-DESIGN.md`, "Calibration after
//! implementation"). This module is the harness only: it constructs a
//! [`KernelNativeSearchOpponentV1`] at a caller-specified tier and seed,
//! drives it against a real, store-loaded checkpoint through
//! [`run_native_checkpoint_with_search_opponent_eval_v1`] (the only entry
//! point that can install a search opponent), and emits a JSON outcome
//! artifact. It runs no panel itself and tunes nothing from outcomes; the
//! coordinator selects which screen to run and reviews every result.
//!
//! ## Entry point convention
//!
//! This follows the codebase's existing `--ignored` native-test launcher
//! convention (`ladder_head_to_head_eval_v1`,
//! `native_science_loop_v1::windows_science_loop_tests`), not a `bin`
//! target. Reasons: (1) every other checkpoint-runner-driven measurement
//! probe in this codebase (H2H, mirror-seat, response-exploiter,
//! pathfinding) is an `--ignored` test invoked by exact name from
//! PowerShell, driven entirely by environment variables and built through
//! `cargo test --release --lib --no-run`; a `bin` target would be a new,
//! second launch convention for no functional gain. (2) the only existing
//! `[[bin]]` target (`mtg-kernel-native`) is gated behind the
//! `native-training-store-v2-production` feature and is the live science
//! loop's own production entry point -- reusing or extending it would blur
//! exactly the eval-only/production boundary the design's stop condition
//! depends on. (3) the `--ignored` test can stay `#[cfg(test)]`, which is
//! what keeps [`run_native_checkpoint_with_search_opponent_eval_v1`] itself
//! unreachable from a non-test build; a `bin` target would force that
//! function out of `cfg(test)` to be callable, which is the one gate this
//! branch must not weaken.
//!
//! ## CardDB / rev3-gate finding
//!
//! Verified directly against this worktree (branch
//! `fable/searcher-calibration-runner-v1`, forked from
//! `fable/searcher-stage2-integration-v1` at `98df7dd`): running
//! `cargo test --lib native_training_store_run_v2::tests::
//! production_owners_and_record_are_independently_pinned_to_frozen_rev3`
//! panics with `TrainRunV2Error { kind: InvalidLiteral }` at the test's own
//! `validate_frozen_rev3_authorities_v2().unwrap()` call. That function
//! (native_training_store_run_v2.rs) runs unconditionally at the top of
//! `validate_decoded_train_run_v2`, the single choke point behind BOTH
//! `decode_train_run_v2` (the public decode entry point) and the internal
//! `validate_train_run_record_v2` construction seam -- there is no second,
//! separate path that constructs a `ValidatedTrainRunV2`. The failing
//! comparison is `KERNEL_CARDDB_HASH != FROZEN_CARD_DB_HASH_U64_V2`
//! (live `0x64c8_2a26_1e07_8f1a` per `card_def.rs`'s own
//! `card_db_hash_v32_is_frozen` test vs. the frozen rev3 literal
//! `0xa06f_a956_6106_f0ea`), together with a runtime-catalog SHA mismatch.
//!
//! The task that produced this module asked whether this blocks only
//! checkpoint-fixture tests or also real Store loads, on the stated
//! expectation that real loads should be unaffected ("it binds a frozen
//! rev3 fixture gate, not live population-store authority"). That
//! expectation does not hold: the check is entirely input-independent (it
//! compares live build-time constants against frozen literals before any
//! record byte is read), so it rejects a real `run.json` exactly as it
//! rejects a fixture. Confirmed by reading (not just citing) a real caller
//! that reads real bytes rather than a fixture helper:
//! `native_population_runtime_resolution_v1::resolve_population_handles_v1`
//! reads `run.json` from a real Store root via `fs::read` and passes the
//! bytes straight to `decode_train_run_v2`, which fails identically. (That
//! function is not itself `#[cfg(test)]`-annotated; its enclosing module
//! happens to be registered `#[cfg(test)]` in `lib.rs` today, a separate,
//! narrower fact than the input-independence of the gate itself.) Every
//! `decode_train_run_v2` callsite in this crate -- fixture or real,
//! test-only or production -- currently returns `Err`.
//!
//! Severity, also independently confirmed: `native_checkpoint_runner_v1`'s
//! OWN pre-existing `mod tests` (untouched by this branch except for a doc
//! comment) is 8 of 12 tests RED on unmodified `98df7dd` for the identical
//! reason (`cargo test --lib native_checkpoint_runner_v1::` there fails the
//! same `TrainRunV2Error { kind: InvalidLiteral }` in
//! `config_preflight_precedes_payload_decode_and_rejects_incomplete_pairs`,
//! `genuine_checkpoint_runs_complete_paired_native_schedule_repeatably`,
//! `real_k2_s4_trained_checkpoint_runs_and_retains_all_digest_bindings`,
//! `starting_player_unset_reproduces_parent_commit_checkpoint_eval_bytes_v1`,
//! `v2_checkpoint_evaluation_preflights_the_window_then_evaluates_live`,
//! `valid_range_rejects_corrupt_payload_before_any_rollout`,
//! `wide_checkpoint_plays_head_to_head_against_frozen_ladder_opponent_end_to_end`,
//! `wide_checkpoint_runs_a_genuine_evaluation_game_end_to_end`). This is not
//! a defect this branch introduced or could have introduced; it predates
//! this branch entirely.
//!
//! Per the task's own instruction, this module does not attempt to route
//! around the frozen validator (no local patch to
//! `FROZEN_CARD_DB_HASH_U64_V2`, no bypass path). The harness below is
//! built and its fail-closed paths are unit-tested wherever they do not
//! require constructing a `ValidatedTrainRunV2`; the parts that do
//! (loading any real checkpoint, and therefore every calibration screen and
//! the required one-pair smoke test) cannot be exercised on this worktree
//! until whoever owns `FROZEN_CARD_DB_HASH_U64_V2` /
//! `FROZEN_RUNTIME_CATALOG_SHA256_V2` reconciles them with the live CardDB.
//! Confirmed end to end against a real population-campaign store
//! (`D:\mtg-kernel-ladder-pilot-20260725\pool3\primary`, a genuine Store
//! root with a real `run.json`, generation 0): running this module's own
//! `--ignored` launcher against it fails at `load_calibration_checkpoint_v1`
//! with `CalibrationRunnerErrorV1::RunDecode`, before any authority,
//! opponent, or rollout is constructed, and before the outcome JSON path is
//! ever touched -- fail-closed, no partial artifact left behind. This is a
//! pre-existing, worktree-wide defect, not something introduced by or
//! specific to the search-opponent feature.
//!
//! ## Known gap: per-decision search diagnostics
//!
//! The design's calibration section and the task that produced this module
//! both ask for "search diagnostics" (the task's own gloss names
//! "simulation counts") per decision. The given entry point cannot supply
//! them: `KernelNativeSearchOpponentV1::select_action` returns a rich
//! [`KernelNativeSearchDecisionV1`] (`transitions_used`, `simulations`,
//! `tree_node_count`, `root_action_stats`), but the shared async rollout
//! dispatch that `run_native_checkpoint_with_search_opponent_eval_v1`
//! bottoms out in discards everything except the selected index at the
//! call site (`async_flat_scored_rollout_v1.rs`, the `(None, None, true)`
//! search-authority arm: `searcher.select_action(session, decision)
//! .map(|result| result.selected_index)...`). That dispatch is deep, shared
//! rollout machinery also used by the live science loop; widening it to
//! surface per-decision search internals is out of this harness's scope
//! ("harness only") and is not attempted here. The outcome artifact below
//! reports what IS honestly observable through the given entry point
//! instead: the tier's compile-bound `transition_budget` /
//! `policy_step_depth_cap` (the search-authority record, embedded in full),
//! plus externally measured wall time, decisions/second, and a coarse mean
//! per-decision latency -- and says so explicitly in its own
//! `known_limitations` field, rather than fabricating a per-decision
//! simulation count.

use crate::kernel_native_search_opponent_v1::{
    KernelNativeSearchAuthorityV1, KernelNativeSearchErrorV1, KernelNativeSearchOpponentV1,
    KernelNativeSearchTierV1, KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1,
};
use crate::native_checkpoint_runner_v1::{
    run_native_checkpoint_with_search_opponent_eval_v1, NativeCheckpointRunResultV1,
    NativeCheckpointRunnerConfigV1, NativeCheckpointRunnerErrorV1,
};
use crate::native_training_store_digest_v1::{lower_hex_raw32_v1, sha256_v1};
use crate::native_training_store_resume_v2::load_native_training_boundary_v2;
use crate::native_training_store_root_v2::ValidatedNativeTrainingStoreRootV2;
use crate::native_training_store_run_v2::{
    decode_train_run_v2, NativeRunEnvironmentTrajectoryContractV1, ValidatedTrainRunV2,
};
use crate::rl::{terminal_tuple_is_valid_v1, PlayerSeatV1};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) const KERNEL_NATIVE_SEARCH_CALIBRATION_OUTCOME_SCHEMA_V1: &str =
    "mtg-kernel-kernel-native-search-calibration-outcome/v1";

/// Default per-call scheduler timeout, matching the existing H2H harness's
/// own choice (`native_science_loop_v1::windows_science_loop_tests::
/// ladder_head_to_head_eval_v1`).
const KERNEL_NATIVE_SEARCH_CALIBRATION_DEFAULT_TIMEOUT_SECONDS_V1: u64 = 3_600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CalibrationRunnerErrorV1 {
    UnrecognizedTier,
    UnauthorizedBaseSeed,
    InvalidAuthority,
    InvalidPairRange,
    EnvironmentContractMismatch,
    Runner(NativeCheckpointRunnerErrorV1),
    RunDecode,
    StoreRootOpen,
    BoundaryLoad,
    ProtocolMismatch,
    OutcomeIo,
}

impl std::fmt::Display for CalibrationRunnerErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CalibrationRunnerErrorV1 {}

impl From<KernelNativeSearchErrorV1> for CalibrationRunnerErrorV1 {
    fn from(_: KernelNativeSearchErrorV1) -> Self {
        Self::InvalidAuthority
    }
}

impl From<NativeCheckpointRunnerErrorV1> for CalibrationRunnerErrorV1 {
    fn from(error: NativeCheckpointRunnerErrorV1) -> Self {
        Self::Runner(error)
    }
}

/// Fail-closed env-var-string-to-tier resolution. Accepts exactly the four
/// tier names the registered "arm-script seed whitelist" layer already uses
/// (`kernel_native_search_opponent_v1::tests::
/// kernel_native_search_diagnostic_env_surface_is_registered_and_fails_closed`,
/// `scripts/experiments/kernel_native_search_opponent_v1/
/// run-diagnostic-registration-smoke.ps1`'s `-Tier` `ValidateSet`):
/// `"T512"`, `"T2048"`, `"T8192"`, `"T32768"`. Deliberately matches that
/// existing registration surface's naming rather than the design
/// document's bare transition-budget numerals, so this feature area has one
/// tier-string convention, not two. Every other input, including a
/// numeric-looking but unregistered budget, is rejected rather than
/// defaulted.
pub(crate) fn resolve_calibration_tier_v1(
    value: &str,
) -> Result<KernelNativeSearchTierV1, CalibrationRunnerErrorV1> {
    match value {
        "T512" => Ok(KernelNativeSearchTierV1::T512),
        "T2048" => Ok(KernelNativeSearchTierV1::T2048),
        "T8192" => Ok(KernelNativeSearchTierV1::T8192),
        "T32768" => Ok(KernelNativeSearchTierV1::T32768),
        _ => Err(CalibrationRunnerErrorV1::UnrecognizedTier),
    }
}

/// Fail-closed base-seed check ahead of authority construction. This is a
/// harness-level, cheap, early rejection of the same
/// `KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1` allowlist
/// `KernelNativeSearchAuthorityV1::validate` enforces -- deliberately
/// checked twice at two different call depths (matching this codebase's own
/// "checklist item 1 scope" pattern for the embedded `action_seed`), so a
/// caller gets a clear, harness-level error before any authority or
/// opponent is even constructed.
pub(crate) fn resolve_calibration_base_seed_v1(
    value: u64,
) -> Result<u64, CalibrationRunnerErrorV1> {
    if KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1.contains(&value) {
        Ok(value)
    } else {
        Err(CalibrationRunnerErrorV1::UnauthorizedBaseSeed)
    }
}

/// Constructs the search authority and opponent for one calibration screen.
/// `base_seed` serves both roles the design's calibration section gives it:
/// the authority's embedded `action_seed` AND (by the caller, separately)
/// the checkpoint runner's `evaluation_base_seed` -- the four calibration
/// seeds (1,987,001 / 1,988,001 / 1,989,001 / 1,990,001) are one number
/// each, not two.
///
/// Always binds `DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2`: every
/// calibration screen this harness runs is under environment-randomization-
/// v2 by design-section instruction ("Drive N seat-swapped CRN pairs...
/// under environment-randomization-v2"), so this is fixed, not a knob. A
/// checkpoint whose own run predates environment-randomization-v2 is
/// rejected earlier, by [`require_environment_randomization_v2_v1`], before
/// this function is ever reached.
pub(crate) fn resolve_calibration_search_opponent_v1(
    tier: KernelNativeSearchTierV1,
    base_seed: u64,
) -> Result<Arc<KernelNativeSearchOpponentV1>, CalibrationRunnerErrorV1> {
    let base_seed = resolve_calibration_base_seed_v1(base_seed)?;
    let authority = KernelNativeSearchAuthorityV1::current(
        tier,
        base_seed,
        crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2,
    )?;
    opponent_from_authority_v1(authority)
}

/// The harness's own opponent-construction boundary, factored out so the
/// "tampered authority" fail-closed path is directly testable: feed it an
/// authority built OUTSIDE `resolve_calibration_search_opponent_v1` (every
/// field of `KernelNativeSearchAuthorityV1` is `pub`, so a caller can build
/// or mutate one directly) and confirm construction is rejected the same
/// way it would be for a live tampering attempt.
pub(crate) fn opponent_from_authority_v1(
    authority: KernelNativeSearchAuthorityV1,
) -> Result<Arc<KernelNativeSearchOpponentV1>, CalibrationRunnerErrorV1> {
    Ok(Arc::new(KernelNativeSearchOpponentV1::new(authority)?))
}

/// Fail-closed pair-range check: both values must be nonzero-count, in
/// range, and even-aligned once doubled into episode indices (the checkpoint
/// runner's own `NativeCheckpointRunnerConfigV1` contract), checked here so
/// a bad range is rejected before any checkpoint is loaded.
pub(crate) fn resolve_calibration_episode_range_v1(
    first_pair_index: u64,
    pair_count: u64,
) -> Result<(u64, u64), CalibrationRunnerErrorV1> {
    if pair_count == 0 {
        return Err(CalibrationRunnerErrorV1::InvalidPairRange);
    }
    let first_episode_index = first_pair_index
        .checked_mul(2)
        .ok_or(CalibrationRunnerErrorV1::InvalidPairRange)?;
    let episode_count = pair_count
        .checked_mul(2)
        .ok_or(CalibrationRunnerErrorV1::InvalidPairRange)?;
    Ok((first_episode_index, episode_count))
}

/// Fail-closed environment-contract check: the design instructs every
/// calibration screen to run "under environment-randomization-v2"; a
/// checkpoint whose own run is on the legacy contract is rejected here,
/// eagerly, rather than surfacing as an opaque `InvalidDecision` deep inside
/// the search authority's per-decision diagnostic-identity check.
pub(crate) fn require_environment_randomization_v2_v1(
    run: &ValidatedTrainRunV2,
) -> Result<(), CalibrationRunnerErrorV1> {
    match run.environment_trajectory_contract_v1() {
        NativeRunEnvironmentTrajectoryContractV1::EnvironmentRandomizationV2 => Ok(()),
        NativeRunEnvironmentTrajectoryContractV1::LegacyV1 => {
            Err(CalibrationRunnerErrorV1::EnvironmentContractMismatch)
        }
    }
}

/// Peak process working set in bytes, measured once at the end of a
/// calibration screen via a single `GetProcessMemoryInfo` call (the OS
/// maintains `PeakWorkingSetSize` continuously from process start, so one
/// call at the end is exact -- no polling loop, no external process
/// needed). `None` on non-Windows or if the OS call fails; this is
/// measurement evidence only, never a decision input.
pub(crate) fn peak_working_set_bytes_v1() -> Option<u64> {
    windows_peak_memory_v1::peak_working_set_bytes_v1()
}

#[cfg(windows)]
mod windows_peak_memory_v1 {
    use std::ffi::c_void;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCountersV1 {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }

    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCountersV1,
            size: u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
    }

    pub(super) fn peak_working_set_bytes_v1() -> Option<u64> {
        let mut counters = ProcessMemoryCountersV1 {
            cb: u32::try_from(std::mem::size_of::<ProcessMemoryCountersV1>()).ok()?,
            PageFaultCount: 0,
            PeakWorkingSetSize: 0,
            WorkingSetSize: 0,
            QuotaPeakPagedPoolUsage: 0,
            QuotaPagedPoolUsage: 0,
            QuotaPeakNonPagedPoolUsage: 0,
            QuotaNonPagedPoolUsage: 0,
            PagefileUsage: 0,
            PeakPagefileUsage: 0,
        };
        // SAFETY: `GetCurrentProcess` returns a pseudo-handle that needs no
        // closing; `counters` is a plain-old-data `#[repr(C)]` struct sized
        // and zero-initialized before the call, matching the Win32 contract
        // for `PROCESS_MEMORY_COUNTERS`.
        let succeeded = unsafe {
            let process = GetCurrentProcess();
            GetProcessMemoryInfo(process, &mut counters, counters.cb) != 0
        };
        succeeded.then_some(counters.PeakWorkingSetSize as u64)
    }
}

#[cfg(not(windows))]
mod windows_peak_memory_v1 {
    pub(super) fn peak_working_set_bytes_v1() -> Option<u64> {
        None
    }
}

/// One CRN leg's deterministic outcome: everything that MUST repeat
/// byte-identically across two runs of the same pair range. Deliberately
/// excludes any measurement-only field (latency, wall time); those go on
/// the outer artifact instead, so `repeat_hash` (computed over exactly this
/// shape) is unaffected by machine load or scheduling jitter between runs.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct CalibrationLegOutcomeV1 {
    pub(crate) episode_index: u64,
    pub(crate) candidate_seat: &'static str,
    pub(crate) winner_seat: Option<&'static str>,
    pub(crate) candidate_reward: i32,
    pub(crate) policy_step_count: u64,
    pub(crate) physical_decision_count: u64,
    pub(crate) opponent_physical_decision_count: u64,
    pub(crate) trajectory_sha256: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct CalibrationPairOutcomeV1 {
    pub(crate) pair_index: u64,
    pub(crate) environment_seed: u64,
    pub(crate) leg_candidate_p0: CalibrationLegOutcomeV1,
    pub(crate) leg_candidate_p1: CalibrationLegOutcomeV1,
    /// Candidate (checkpoint-under-test) net reward across both legs of
    /// this pair: `-2..=2`. The design's own H2H sibling labels the
    /// equivalent "net positive per pair" quantity a diagnostic, never a
    /// gate quantity; the same discipline applies here -- this harness
    /// makes no promotion or gate decision.
    pub(crate) candidate_paired_net: i32,
}

fn seat_label_v1(seat: PlayerSeatV1) -> &'static str {
    match seat {
        PlayerSeatV1::P0 => "P0",
        PlayerSeatV1::P1 => "P1",
    }
}

/// Assembles one leg's outcome from the matched runner episode binding and
/// rollout episode, cross-checking the terminal tuple's internal
/// consistency the same way the H2H harness does
/// (`terminal_tuple_is_valid_v1`) before trusting any of its fields.
fn leg_outcome_v1(
    binding: &crate::native_checkpoint_runner_v1::NativeCheckpointRunnerEpisodeV1,
    episode: &crate::async_rollout::AsyncRolloutEpisodeV1,
) -> Result<CalibrationLegOutcomeV1, CalibrationRunnerErrorV1> {
    if episode.terminal.episode_id != binding.episode_index() {
        return Err(CalibrationRunnerErrorV1::ProtocolMismatch);
    }
    if !terminal_tuple_is_valid_v1(
        episode.terminal.terminal_outcome,
        episode.terminal.terminal_classification,
        episode.terminal.winner,
        episode.terminal.terminal_reward,
    ) {
        return Err(CalibrationRunnerErrorV1::ProtocolMismatch);
    }
    let candidate_seat = binding.learner_seat();
    let seat_index = match candidate_seat {
        PlayerSeatV1::P0 => 0_usize,
        PlayerSeatV1::P1 => 1_usize,
    };
    Ok(CalibrationLegOutcomeV1 {
        episode_index: binding.episode_index(),
        candidate_seat: seat_label_v1(candidate_seat),
        winner_seat: episode.terminal.winner.map(seat_label_v1),
        candidate_reward: episode.terminal.terminal_reward[seat_index],
        policy_step_count: binding.policy_step_count(),
        physical_decision_count: binding.physical_decision_count(),
        opponent_physical_decision_count: binding.opponent_physical_decision_count(),
        trajectory_sha256: lower_hex_raw32_v1(binding.trajectory_sha256()),
    })
}

/// Builds the ordered per-pair outcome sequence from one successful runner
/// result. Requires `episode_bindings` and `rollout().episodes` to name the
/// exact same episode-index set (already enforced upstream by the runner's
/// own observer, re-checked here defensively) and requires every episode
/// index to be even-aligned into `[2k, 2k+1]` pairs with the candidate
/// seat alternating P0/P1 exactly as `native_trainer_episode_schedule_v1`
/// derives it -- the SAME pair-seed derivation the H2H harness uses, reused
/// unmodified via the shared checkpoint runner rather than reimplemented
/// here.
pub(crate) fn pair_outcomes_v1(
    result: &NativeCheckpointRunResultV1,
) -> Result<Vec<CalibrationPairOutcomeV1>, CalibrationRunnerErrorV1> {
    let bindings = result.episode_bindings();
    let episodes = &result.rollout().episodes;
    if bindings.len() != episodes.len() || bindings.is_empty() || !bindings.len().is_multiple_of(2)
    {
        return Err(CalibrationRunnerErrorV1::ProtocolMismatch);
    }
    let mut episodes_by_id = std::collections::BTreeMap::new();
    for episode in episodes {
        if episodes_by_id
            .insert(episode.terminal.episode_id, episode)
            .is_some()
        {
            return Err(CalibrationRunnerErrorV1::ProtocolMismatch);
        }
    }
    let mut sorted_bindings: Vec<_> = bindings.iter().collect();
    sorted_bindings.sort_by_key(|binding| binding.episode_index());
    let mut pairs = Vec::with_capacity(sorted_bindings.len() / 2);
    for leg_pair in sorted_bindings.chunks_exact(2) {
        let [binding_p0, binding_p1] = leg_pair else {
            return Err(CalibrationRunnerErrorV1::ProtocolMismatch);
        };
        if binding_p0.episode_index() % 2 != 0
            || binding_p1.episode_index() != binding_p0.episode_index() + 1
            || binding_p0.learner_seat() != PlayerSeatV1::P0
            || binding_p1.learner_seat() != PlayerSeatV1::P1
            || binding_p0.environment_seed() != binding_p1.environment_seed()
        {
            return Err(CalibrationRunnerErrorV1::ProtocolMismatch);
        }
        let episode_p0 = episodes_by_id
            .get(&binding_p0.episode_index())
            .ok_or(CalibrationRunnerErrorV1::ProtocolMismatch)?;
        let episode_p1 = episodes_by_id
            .get(&binding_p1.episode_index())
            .ok_or(CalibrationRunnerErrorV1::ProtocolMismatch)?;
        let leg_candidate_p0 = leg_outcome_v1(binding_p0, episode_p0)?;
        let leg_candidate_p1 = leg_outcome_v1(binding_p1, episode_p1)?;
        let candidate_paired_net =
            leg_candidate_p0.candidate_reward + leg_candidate_p1.candidate_reward;
        pairs.push(CalibrationPairOutcomeV1 {
            pair_index: binding_p0.episode_index() / 2,
            environment_seed: binding_p0.environment_seed(),
            leg_candidate_p0,
            leg_candidate_p1,
            candidate_paired_net,
        });
    }
    Ok(pairs)
}

/// SHA-256 over the canonical JSON encoding of the ordered per-pair outcome
/// sequence only (never the measurement-only fields around it). Two runs of
/// the identical pair range that produce the identical byte sequence here
/// have proven the design's determinism requirement
/// ("repeats must be byte-identical"); a differing hash is a determinism
/// failure regardless of what wall time or peak memory the two runs report.
pub(crate) fn repeat_hash_v1(
    pairs: &[CalibrationPairOutcomeV1],
) -> Result<String, CalibrationRunnerErrorV1> {
    let bytes = serde_json::to_vec(pairs).map_err(|_| CalibrationRunnerErrorV1::OutcomeIo)?;
    Ok(lower_hex_raw32_v1(sha256_v1(&bytes)))
}

/// Loads one checkpoint (run + specific generation) from a full Store root,
/// exactly the way the H2H harness loads its opponent side (digest
/// re-validation optional, eval-only tooling): `decode_train_run_v2` on the
/// root's own `run.json`, then a validated boundary walk to the pinned
/// generation. Requires the generation to be named explicitly (no
/// latest-loading fallback) -- the H2H harness's own doc comment records why
/// pinning is mandatory practice here (the promoted(2) latest-vs-pinned
/// mismeasurement disclosure).
pub(crate) fn load_calibration_checkpoint_v1(
    store_root: &std::path::Path,
    generation: u64,
) -> Result<
    (
        ValidatedTrainRunV2,
        crate::native_training_store_checkpoint_v3::CheckpointManifestV3,
        Vec<u8>,
    ),
    CalibrationRunnerErrorV1,
> {
    let run_bytes = std::fs::read(store_root.join("run.json"))
        .map_err(|_| CalibrationRunnerErrorV1::RunDecode)?;
    let run = decode_train_run_v2(&run_bytes).map_err(|_| CalibrationRunnerErrorV1::RunDecode)?;
    require_environment_randomization_v2_v1(&run)?;
    let root = ValidatedNativeTrainingStoreRootV2::open_v2(store_root)
        .map_err(|_| CalibrationRunnerErrorV1::StoreRootOpen)?;
    let boundary = load_native_training_boundary_v2(&root, &run, generation)
        .map_err(|_| CalibrationRunnerErrorV1::BoundaryLoad)?;
    let (checkpoint, payload) = boundary.into_checkpoint_and_payload();
    Ok((run, checkpoint, payload))
}

/// Full configuration for one calibration screen invocation, resolved from
/// environment variables by the `--ignored` test below.
pub(crate) struct CalibrationScreenConfigV1 {
    pub(crate) tier: KernelNativeSearchTierV1,
    pub(crate) tier_label: String,
    pub(crate) base_seed: u64,
    pub(crate) checkpoint_store_root: std::path::PathBuf,
    pub(crate) checkpoint_generation: u64,
    pub(crate) first_pair_index: u64,
    pub(crate) pair_count: u64,
    pub(crate) scheduler_timeout: Duration,
}

/// Runs one calibration screen end to end: loads the checkpoint, builds the
/// search authority and opponent, calls the checkpoint runner while timing
/// it, measures peak working set, assembles the per-pair outcome sequence,
/// and returns the finished JSON artifact value (the caller writes it out).
///
/// No wall clock participates in any decision: `Instant` sampling below
/// wraps the ENTIRE deterministic call and is read only after it returns,
/// exactly as the design requires ("timing is measurement only").
pub(crate) fn run_calibration_screen_v1(
    config: CalibrationScreenConfigV1,
) -> Result<serde_json::Value, CalibrationRunnerErrorV1> {
    let (first_episode_index, episode_count) =
        resolve_calibration_episode_range_v1(config.first_pair_index, config.pair_count)?;
    let search_opponent = resolve_calibration_search_opponent_v1(config.tier, config.base_seed)?;
    let authority = search_opponent.authority().clone();
    let authority_digest = authority
        .digest()
        .map_err(|_| CalibrationRunnerErrorV1::InvalidAuthority)?;
    let (run, checkpoint, checkpoint_payload) = load_calibration_checkpoint_v1(
        &config.checkpoint_store_root,
        config.checkpoint_generation,
    )?;

    let runner_config = NativeCheckpointRunnerConfigV1 {
        evaluation_base_seed: config.base_seed,
        first_episode_index,
        episode_count,
        scheduler_timeout: config.scheduler_timeout,
        measure_broker_service_time: false,
        starting_player: None,
    };

    let clock = Instant::now();
    let result = run_native_checkpoint_with_search_opponent_eval_v1(
        &run,
        &checkpoint,
        &checkpoint_payload,
        runner_config,
        search_opponent,
    )?;
    let wall_time = clock.elapsed();
    let peak_working_set = peak_working_set_bytes_v1();

    if !result.rollout().all_natural() {
        return Err(CalibrationRunnerErrorV1::ProtocolMismatch);
    }
    let pairs = pair_outcomes_v1(&result)?;
    let repeat_hash = repeat_hash_v1(&pairs)?;

    let total_opponent_physical_decisions: u64 = pairs
        .iter()
        .map(|pair| {
            pair.leg_candidate_p0.opponent_physical_decision_count
                + pair.leg_candidate_p1.opponent_physical_decision_count
        })
        .sum();
    let total_physical_decisions: u64 = pairs
        .iter()
        .map(|pair| {
            pair.leg_candidate_p0.physical_decision_count
                + pair.leg_candidate_p1.physical_decision_count
        })
        .sum();
    let wall_seconds = wall_time.as_secs_f64();
    let opponent_decisions_per_second = if wall_seconds > 0.0 {
        total_opponent_physical_decisions as f64 / wall_seconds
    } else {
        0.0
    };
    let mean_opponent_decision_latency_ms = if total_opponent_physical_decisions > 0 {
        Some((wall_seconds * 1_000.0) / total_opponent_physical_decisions as f64)
    } else {
        None
    };

    let candidate_wins = pairs
        .iter()
        .flat_map(|pair| {
            [
                pair.leg_candidate_p0.candidate_reward,
                pair.leg_candidate_p1.candidate_reward,
            ]
        })
        .filter(|&reward| reward == 1)
        .count();
    let candidate_losses = pairs
        .iter()
        .flat_map(|pair| {
            [
                pair.leg_candidate_p0.candidate_reward,
                pair.leg_candidate_p1.candidate_reward,
            ]
        })
        .filter(|&reward| reward == -1)
        .count();
    let candidate_draws = pairs
        .iter()
        .flat_map(|pair| {
            [
                pair.leg_candidate_p0.candidate_reward,
                pair.leg_candidate_p1.candidate_reward,
            ]
        })
        .filter(|&reward| reward == 0)
        .count();
    let candidate_paired_net_total: i32 = pairs.iter().map(|pair| pair.candidate_paired_net).sum();

    let tier_label = config.tier_label;
    Ok(serde_json::json!({
        "schema": KERNEL_NATIVE_SEARCH_CALIBRATION_OUTCOME_SCHEMA_V1,
        "tier": tier_label,
        "base_seed": config.base_seed,
        "authority": authority,
        "authority_sha256": lower_hex_raw32_v1(authority_digest),
        "checkpoint": {
            "run_sha256": lower_hex_raw32_v1(result.run_sha256()),
            "identity_bundle_sha256": lower_hex_raw32_v1(result.identity_bundle_sha256()),
            "checkpoint_manifest_sha256": lower_hex_raw32_v1(result.checkpoint_manifest_sha256()),
            "checkpoint_payload_sha256": lower_hex_raw32_v1(result.checkpoint_payload_sha256()),
            "model_parameter_sha256": lower_hex_raw32_v1(result.model_parameter_sha256()),
            "generation_index": result.generation_index(),
            "requested_generation": config.checkpoint_generation,
        },
        "runtime": {
            "worker_count": result.worker_count(),
            "sessions_per_worker": result.sessions_per_worker(),
            "broker_batch_target": result.broker_batch_target(),
            "environment_contract": "environment_randomization_v2",
            "all_natural": true,
        },
        "evaluation": {
            "first_pair_index": config.first_pair_index,
            "pair_count": config.pair_count,
            "episode_count": episode_count,
        },
        "outcomes": {
            "candidate": {"wins": candidate_wins, "losses": candidate_losses, "draws": candidate_draws},
            "search_opponent": {"wins": candidate_losses, "losses": candidate_wins, "draws": candidate_draws},
            "candidate_paired_net_total": candidate_paired_net_total,
        },
        "timing": {
            "wall_seconds": wall_seconds,
            "opponent_decisions_per_second": opponent_decisions_per_second,
            "mean_opponent_decision_latency_ms": mean_opponent_decision_latency_ms,
            "total_opponent_physical_decisions": total_opponent_physical_decisions,
            "total_physical_decisions": total_physical_decisions,
        },
        "peak_memory": {
            "peak_working_set_bytes": peak_working_set,
            "measured": peak_working_set.is_some(),
        },
        "pairs": pairs,
        "repeat_hash_sha256": repeat_hash,
        "known_limitations": [
            "per-decision search-tree diagnostics (simulations, tree_node_count, \
             root_action_stats) are not observable through this entry point: the \
             shared async rollout dispatch discards KernelNativeSearchDecisionV1 \
             down to its selected_index at the search-authority call site; only \
             the tier's compile-bound transition_budget/policy_step_depth_cap \
             (in `authority`) and externally measured wall-clock timing are \
             reported here.",
            "mean_opponent_decision_latency_ms is a single run-wide mean, not a \
             per-decision distribution: per-decision timestamps are not exposed \
             by the rollout dispatch either.",
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_resolution_accepts_exactly_the_four_registered_tier_names() {
        assert_eq!(
            resolve_calibration_tier_v1("T512"),
            Ok(KernelNativeSearchTierV1::T512)
        );
        assert_eq!(
            resolve_calibration_tier_v1("T2048"),
            Ok(KernelNativeSearchTierV1::T2048)
        );
        assert_eq!(
            resolve_calibration_tier_v1("T8192"),
            Ok(KernelNativeSearchTierV1::T8192)
        );
        assert_eq!(
            resolve_calibration_tier_v1("T32768"),
            Ok(KernelNativeSearchTierV1::T32768)
        );
    }

    #[test]
    fn tier_resolution_fails_closed_on_every_unregistered_string() {
        for bad in [
            "", "1024", "512", "t512", "T512 ", " T512", "T0", "T-512", "T65536", "T5120",
        ] {
            assert_eq!(
                resolve_calibration_tier_v1(bad),
                Err(CalibrationRunnerErrorV1::UnrecognizedTier),
                "unexpected acceptance of {bad:?}"
            );
        }
    }

    #[test]
    fn base_seed_resolution_accepts_exactly_the_four_calibration_seeds() {
        for seed in KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1 {
            assert_eq!(resolve_calibration_base_seed_v1(seed), Ok(seed));
        }
    }

    #[test]
    fn base_seed_resolution_fails_closed_on_unregistered_seeds() {
        for bad in [0, 1, 1_987_000, 1_987_002, 1_990_002, u64::MAX] {
            assert_eq!(
                resolve_calibration_base_seed_v1(bad),
                Err(CalibrationRunnerErrorV1::UnauthorizedBaseSeed)
            );
        }
    }

    #[test]
    fn search_opponent_resolution_fails_closed_on_unregistered_seed_before_authority_construction()
    {
        let error = resolve_calibration_search_opponent_v1(KernelNativeSearchTierV1::T512, 42)
            .expect_err("seed 42 is not in the calibration allowlist");
        assert_eq!(error, CalibrationRunnerErrorV1::UnauthorizedBaseSeed);
    }

    #[test]
    fn search_opponent_resolution_succeeds_for_every_tier_at_every_authorized_seed() {
        for tier in [
            KernelNativeSearchTierV1::T512,
            KernelNativeSearchTierV1::T2048,
            KernelNativeSearchTierV1::T8192,
            KernelNativeSearchTierV1::T32768,
        ] {
            for seed in KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1 {
                resolve_calibration_search_opponent_v1(tier, seed)
                    .unwrap_or_else(|error| panic!("tier={tier:?} seed={seed} failed: {error}"));
            }
        }
    }

    fn valid_authority_v1() -> KernelNativeSearchAuthorityV1 {
        KernelNativeSearchAuthorityV1::current(
            KernelNativeSearchTierV1::T512,
            KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1[0],
            crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM_ENVIRONMENT_V2,
        )
        .unwrap()
    }

    #[test]
    fn opponent_construction_accepts_a_genuine_authority() {
        opponent_from_authority_v1(valid_authority_v1()).expect("genuine authority must construct");
    }

    #[test]
    fn opponent_construction_fails_closed_on_every_tampered_authority_field() {
        macro_rules! assert_tamper_rejected {
            ($field:ident, $value:expr) => {{
                let mut tampered = valid_authority_v1();
                tampered.$field = $value;
                let error = opponent_from_authority_v1(tampered).expect_err(concat!(
                    "tampered ",
                    stringify!($field),
                    " must be rejected"
                ));
                assert_eq!(error, CalibrationRunnerErrorV1::InvalidAuthority);
            }};
        }
        assert_tamper_rejected!(action_seed, 1_987_000);
        assert_tamper_rejected!(transition_budget, 511);
        assert_tamper_rejected!(schema, "tampered-schema/v1".to_owned());
        assert_tamper_rejected!(algorithm_identity, "tampered-algorithm/v1".to_owned());
        assert_tamper_rejected!(card_db_hash, 0);
        assert_tamper_rejected!(private_diagnostic_identity, "tampered".to_owned());
    }

    #[test]
    fn episode_range_resolution_is_pair_indexed_and_fails_closed_on_zero_pairs() {
        assert_eq!(resolve_calibration_episode_range_v1(0, 16), Ok((0, 32)));
        assert_eq!(resolve_calibration_episode_range_v1(4, 1), Ok((8, 2)));
        assert_eq!(
            resolve_calibration_episode_range_v1(0, 0),
            Err(CalibrationRunnerErrorV1::InvalidPairRange)
        );
    }

    #[test]
    fn episode_range_resolution_fails_closed_on_overflow() {
        assert_eq!(
            resolve_calibration_episode_range_v1(u64::MAX, 1),
            Err(CalibrationRunnerErrorV1::InvalidPairRange)
        );
        assert_eq!(
            resolve_calibration_episode_range_v1(0, u64::MAX),
            Err(CalibrationRunnerErrorV1::InvalidPairRange)
        );
    }

    #[test]
    fn repeat_hash_is_stable_across_independently_built_identical_sequences() {
        let leg = |episode_index: u64, seat: &'static str, reward: i32| CalibrationLegOutcomeV1 {
            episode_index,
            candidate_seat: seat,
            winner_seat: Some(seat),
            candidate_reward: reward,
            policy_step_count: 10,
            physical_decision_count: 20,
            opponent_physical_decision_count: 8,
            trajectory_sha256: "ab".repeat(32),
        };
        let build = || {
            vec![CalibrationPairOutcomeV1 {
                pair_index: 0,
                environment_seed: 42,
                leg_candidate_p0: leg(0, "P0", 1),
                leg_candidate_p1: leg(1, "P1", -1),
                candidate_paired_net: 0,
            }]
        };
        assert_eq!(
            repeat_hash_v1(&build()).unwrap(),
            repeat_hash_v1(&build()).unwrap()
        );
    }

    #[test]
    fn repeat_hash_changes_when_a_single_reward_changes() {
        let leg = |reward: i32| CalibrationLegOutcomeV1 {
            episode_index: 0,
            candidate_seat: "P0",
            winner_seat: Some("P0"),
            candidate_reward: reward,
            policy_step_count: 10,
            physical_decision_count: 20,
            opponent_physical_decision_count: 8,
            trajectory_sha256: "ab".repeat(32),
        };
        let other_leg = CalibrationLegOutcomeV1 {
            episode_index: 1,
            candidate_seat: "P1",
            winner_seat: Some("P0"),
            candidate_reward: -1,
            policy_step_count: 10,
            physical_decision_count: 20,
            opponent_physical_decision_count: 8,
            trajectory_sha256: "ab".repeat(32),
        };
        let a = vec![CalibrationPairOutcomeV1 {
            pair_index: 0,
            environment_seed: 42,
            leg_candidate_p0: leg(1),
            leg_candidate_p1: other_leg.clone(),
            candidate_paired_net: 0,
        }];
        let b = vec![CalibrationPairOutcomeV1 {
            pair_index: 0,
            environment_seed: 42,
            leg_candidate_p0: leg(-1),
            leg_candidate_p1: other_leg,
            candidate_paired_net: -2,
        }];
        assert_ne!(repeat_hash_v1(&a).unwrap(), repeat_hash_v1(&b).unwrap());
    }

    #[test]
    fn peak_working_set_reads_a_positive_value_on_windows() {
        let value = peak_working_set_bytes_v1();
        #[cfg(windows)]
        assert!(
            value.unwrap_or(0) > 0,
            "expected a positive peak working set"
        );
        #[cfg(not(windows))]
        assert_eq!(value, None);
    }
}

/// `--ignored` calibration launcher, matching
/// `native_science_loop_v1::windows_science_loop_tests::
/// ladder_head_to_head_eval_v1`'s exact invocation convention: driven
/// entirely by environment variables, invoked by exact test name from
/// PowerShell against a `cargo test --release --lib --no-run` executable.
///
/// Environment contract (all required unless noted):
/// - `KNS_TIER`: one of `"T512"`, `"T2048"`, `"T8192"`, `"T32768"` (matching
///   the registered `KERNEL_SEARCH_DIAGNOSTIC_TIER` naming, not the design
///   document's bare transition-budget numerals).
/// - `KNS_BASE_SEED`: one of `1987001` (throughput screen), `1988001`
///   (promoted(2) panel), `1989001` (frozen de-novo panel), `1990001`
///   (future CP7 bridge panel, not authorized to run by this harness alone).
///   Serves as both the search authority's `action_seed` and the checkpoint
///   runner's `evaluation_base_seed` (one number, two roles, per the
///   design).
/// - `KNS_CHECKPOINT_STORE_ROOT`: full validated Store root of the
///   checkpoint under evaluation.
/// - `KNS_CHECKPOINT_GEN`: exact pinned generation (no latest-loading
///   fallback).
/// - `KNS_FIRST_PAIR_INDEX`: optional, default `0`.
/// - `KNS_PAIR_COUNT`: pair count (`16` for the throughput screen, `256`
///   for a matched panel, `1` for the smoke test).
/// - `KNS_TIMEOUT_SECONDS`: optional, default `3600`.
/// - `KNS_OUTCOME_JSON`: output path; refuses to overwrite an existing file.
#[cfg(all(test, windows))]
mod windows_calibration_tests {
    use super::*;

    fn required_env_v1(name: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
    }

    #[test]
    #[ignore = "measurement probe, run explicitly"]
    fn kernel_native_search_calibration_screen_v1() {
        let tier_label = required_env_v1("KNS_TIER");
        let tier = resolve_calibration_tier_v1(&tier_label)
            .unwrap_or_else(|error| panic!("KNS_TIER={tier_label:?}: {error}"));
        let base_seed: u64 = required_env_v1("KNS_BASE_SEED")
            .parse()
            .expect("KNS_BASE_SEED must be a u64");
        let checkpoint_store_root =
            std::path::PathBuf::from(required_env_v1("KNS_CHECKPOINT_STORE_ROOT"));
        let checkpoint_generation: u64 = required_env_v1("KNS_CHECKPOINT_GEN")
            .parse()
            .expect("KNS_CHECKPOINT_GEN must be a u64");
        let first_pair_index: u64 = std::env::var("KNS_FIRST_PAIR_INDEX")
            .unwrap_or_else(|_| "0".to_owned())
            .parse()
            .expect("KNS_FIRST_PAIR_INDEX must be a u64");
        let pair_count: u64 = required_env_v1("KNS_PAIR_COUNT")
            .parse()
            .expect("KNS_PAIR_COUNT must be a u64");
        let timeout_seconds: u64 = std::env::var("KNS_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| {
                KERNEL_NATIVE_SEARCH_CALIBRATION_DEFAULT_TIMEOUT_SECONDS_V1.to_string()
            })
            .parse()
            .expect("KNS_TIMEOUT_SECONDS must be a u64");
        let outcome_path = required_env_v1("KNS_OUTCOME_JSON");

        println!(
            "KNS tier={tier_label} base_seed={base_seed} pair_count={pair_count} \
             first_pair_index={first_pair_index}"
        );

        let artifact = run_calibration_screen_v1(CalibrationScreenConfigV1 {
            tier,
            tier_label,
            base_seed,
            checkpoint_store_root,
            checkpoint_generation,
            first_pair_index,
            pair_count,
            scheduler_timeout: Duration::from_secs(timeout_seconds),
        })
        .unwrap_or_else(|error| panic!("calibration screen failed: {error}"));

        let mut bytes =
            serde_json::to_vec_pretty(&artifact).expect("outcome artifact must serialize");
        bytes.push(b'\n');
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&outcome_path)
                .expect("KNS_OUTCOME_JSON must name a create-new file");
            file.write_all(&bytes)
                .expect("outcome artifact must write completely");
            file.sync_all()
                .expect("outcome artifact must reach the filesystem");
        }
        println!(
            "KNS outcome_artifact={} sha256={}",
            std::path::Path::new(&outcome_path).display(),
            lower_hex_raw32_v1(sha256_v1(&bytes))
        );
    }
}
