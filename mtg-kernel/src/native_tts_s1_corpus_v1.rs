//! Test-time-search wrapper, stage S1: the FROZEN stratified decision
//! corpus (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S1:
//! "a FROZEN stratified corpus of >= 512 decisions from seeded self-play
//! (both roles; high branching, stack interaction, combat, late game
//! strata) replayed per tier").
//!
//! This module builds the corpus. It never searches: the corpus is the
//! INPUT to the per-tier replay (`native_tts_s1_replay_v1`), and building
//! it with a search would make the decision population a function of the
//! tier under measurement.
//!
//! # What one candidate is
//!
//! Seeded CPU self-play with BOTH seats driven by the same loaded
//! checkpoint, sampling the policy at temperature 1 (the sampler
//! `fast_sampler::FastCategoricalScratch::sample` has no temperature
//! parameter; temperature 1 is the only behavior it has). Every decision
//! the session presents is a candidate, because with both seats on the
//! same checkpoint every decision is a decision the wrapped agent would
//! itself have to make. The seat a decision belongs to is a stratum, not a
//! filter.
//!
//! Only NATURAL terminals contribute. An episode that hit the decision cap
//! (`TerminalClassificationV1::Truncated`) or failed closed
//! (`::Halted`) is discarded whole, not truncated: its late decisions are
//! drawn from a different population than a real game's, and the late-game
//! stratum is exactly where that would bite.
//!
//! # Replay coordinates
//!
//! A candidate records the coordinates that reconstruct it exactly:
//! the episode's base seed and episode id (which together fix the
//! environment seed through `native_trainer_episode_schedule_v1`, the same
//! function the CP7 scorer's own reset path uses), the decision ordinal
//! within the episode, and the exact ordered flat-action indices that were
//! played to reach it. Reconstruction is therefore a replay of the kernel,
//! not a restored snapshot: nothing about the recorded state is trusted,
//! and the replay tool re-derives it and then proves the reconstructed
//! legal surface matches the recorded one before it searches.
//!
//! # Why the sampling seed is this module's own
//!
//! The trainer's own action-seed derivation
//! (`NativeLaneScheduleStateV1::preflight_action_seed`) is deliberately
//! ASYMMETRIC between the learner seat and the opponent seat: with no
//! ladder member bound, the opponent seat is uniform-sampled rather than
//! policy-sampled. A both-seats-same-checkpoint corpus needs the two seats
//! drawn the same way, so this module derives its own seed under its own
//! domain label, `TTS_S1_CORPUS_POLICY_SAMPLE_DOMAIN_V1`, from the
//! launcher-owned seed block plus the decision's own identity. The seed
//! block is resolved by ID out of
//! `MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1`, so no unregistered
//! literal can reach a corpus through a command line.
//!
//! # Selection
//!
//! `select_tts_s1_corpus_v1` is a pure function of the ordered candidate
//! list. It is separated from the self-play driver precisely so the quota
//! and round-robin rules are testable without a checkpoint.

use crate::canonical_json_v1::{
    from_canonical_json_bytes_v1, to_canonical_json_bytes_v1, CanonicalJsonNullPolicyV1,
};
use crate::durable_move_publication_v2::publish_immutable_file_by_move_v2;
use crate::durable_publication_v1::{
    capture_existing_publication_parent_v1, DurableFileExpectationV1,
};
use crate::fast_sampler::FastCategoricalScratch;
use crate::model_guided_search_authority_v1::authorized_seed_block_v1;
use crate::model_guided_search_outcome_v4::lower_hex_sha256_v4;
use crate::native_checkpoint_shadow_stdio_v1::{
    kernel_phase_step_name_v2, load_checkpoint_v1, player_seat_index_v1,
    ShadowCheckpointAuthorityV1, ShadowCheckpointIdentityV1,
};
use crate::native_trainer_schedule_v1::native_trainer_episode_schedule_v1;
use crate::rl::{PlayerSeatV1, TerminalClassificationV1};
use crate::rl_session::{
    FastActorDecisionKindV1, FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1,
    CANONICAL_RALLY_DECK_ID,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::Path;

/// Wire schema of the frozen corpus manifest.
pub const TTS_S1_CORPUS_SCHEMA_V1: &str = "mtg-kernel-tts-s1-corpus-manifest/v1";

/// The selection rule the manifest commits to, spelled out so a reader
/// never has to infer it from the decision list.
pub const TTS_S1_CORPUS_SELECTION_RULE_V1: &str =
    "quota-fill-in-declared-stratum-order-then-round-robin-over-strata-ascending-seed-order/v1";

/// Domain label for this module's own policy-sampling seed. Distinct from
/// every trainer, scorer, and search seed domain in the crate; see the
/// module docs for why this module derives its own.
pub const TTS_S1_CORPUS_POLICY_SAMPLE_DOMAIN_V1: &str = "mtg-kernel-tts-s1-corpus-policy-sample/v1";

/// Sketch Section 5, S1: "a FROZEN stratified corpus of >= 512 decisions".
pub const TTS_S1_CORPUS_TARGET_DECISIONS_V1: u32 = 512;
/// Both roles: at least this many decisions per acting seat.
pub const TTS_S1_CORPUS_MIN_PER_SEAT_V1: u32 = 96;
/// The four named strata quotas.
pub const TTS_S1_CORPUS_MIN_HIGH_BRANCHING_V1: u32 = 64;
pub const TTS_S1_CORPUS_MIN_STACK_INTERACTION_V1: u32 = 64;
pub const TTS_S1_CORPUS_MIN_COMBAT_V1: u32 = 64;
pub const TTS_S1_CORPUS_MIN_LATE_GAME_V1: u32 = 64;

/// A decision is HIGH BRANCHING at this many legal actions or more.
pub const TTS_S1_HIGH_BRANCHING_MIN_LEGAL_ACTIONS_V1: u32 = 6;
/// A decision is STACK INTERACTION at this stack depth or more.
pub const TTS_S1_STACK_INTERACTION_MIN_DEPTH_V1: u32 = 1;
/// Late game, first clause: the kernel round number.
pub const TTS_S1_LATE_GAME_MIN_KERNEL_TURN_V1: u32 = 8;
/// Late game, second clause: either player's life total.
pub const TTS_S1_LATE_GAME_MAX_LIFE_V1: i32 = 5;

/// A corpus build may not run more episodes than this. A bound, not a
/// tuning knob: it exists so a mis-typed episode count fails closed at
/// configuration time instead of running for a day.
pub const TTS_S1_CORPUS_MAX_EPISODES_V1: u64 = 4_096;

/// The six strata, in the FIXED order the quota fill and the round-robin
/// both walk. Reordering these variants changes the selected corpus, so
/// the order is part of the pre-registered selection rule, not an
/// implementation detail.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsS1StratumV1 {
    SeatP0,
    SeatP1,
    HighBranching,
    StackInteraction,
    Combat,
    LateGame,
}

impl TtsS1StratumV1 {
    /// The declared order. `select_tts_s1_corpus_v1` walks this for the
    /// quota fill and then cycles it for the round-robin.
    pub const DECLARED_ORDER_V1: [Self; 6] = [
        Self::SeatP0,
        Self::SeatP1,
        Self::HighBranching,
        Self::StackInteraction,
        Self::Combat,
        Self::LateGame,
    ];

    /// This stratum's pre-registered minimum.
    pub const fn quota_v1(self) -> u32 {
        match self {
            Self::SeatP0 | Self::SeatP1 => TTS_S1_CORPUS_MIN_PER_SEAT_V1,
            Self::HighBranching => TTS_S1_CORPUS_MIN_HIGH_BRANCHING_V1,
            Self::StackInteraction => TTS_S1_CORPUS_MIN_STACK_INTERACTION_V1,
            Self::Combat => TTS_S1_CORPUS_MIN_COMBAT_V1,
            Self::LateGame => TTS_S1_CORPUS_MIN_LATE_GAME_V1,
        }
    }
}

/// The stratification labels the sketch names, recorded per decision
/// whether or not the decision is selected.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1DecisionLabelsV1 {
    /// Which seat is to act.
    pub seat: PlayerSeatV1,
    /// The kernel's own round counter (`state::GameState::turn`), which is
    /// shared by both players rather than being a per-player turn index.
    pub kernel_turn: u32,
    /// The kernel phase step, in the frozen wire spelling the scorer's own
    /// kernel clock uses.
    pub phase_step: String,
    /// Branching: the size of the ordered legal-action set.
    pub legal_action_count: u32,
    /// Items on the stack at this decision.
    pub stack_depth: u32,
    /// Anywhere inside the combat phase (begin combat through end
    /// combat). This is the sketch's "whether inside combat" LABEL; it is
    /// deliberately broader than the combat STRATUM, which is the two
    /// declaration steps only.
    pub in_combat: bool,
    /// The declare-attackers or declare-blockers step: the combat stratum
    /// membership test itself.
    pub combat_declaration_step: bool,
    /// Kernel turn >= 8, or either life total <= 5.
    pub late_game: bool,
    pub life_p0: i32,
    pub life_p1: i32,
}

/// Everything needed to reconstruct one decision from nothing but the
/// checkpoint and these numbers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1ReplayCoordinatesV1 {
    /// The launcher-owned seed block value the episode schedule was
    /// derived from.
    pub episode_base_seed: u64,
    pub episode_id: u64,
    /// Recorded so a reader can see the schedule's own output without
    /// re-running it; the replay re-derives and compares it.
    pub environment_seed: u64,
    /// Zero-based index of this decision inside its episode.
    pub decision_ordinal: u64,
    /// The exact ordered flat-action indices played to reach this
    /// decision. Length always equals `decision_ordinal`.
    pub action_sequence: Vec<u32>,
}

/// The legal surface as it was observed when the decision was recorded.
/// The replay reconstructs the decision and refuses to search unless every
/// field here matches, so a kernel change that silently moves a decision
/// is a fail-closed error rather than a quietly different measurement.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1RecordedSurfaceV1 {
    pub policy_step: u64,
    pub environment_revision: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub acting_player: PlayerSeatV1,
    pub decision_kind: String,
    pub legal_action_count: u32,
    /// `FastActorSessionV1::diagnostic_state_hash`.
    pub diagnostic_state_hash_u64_hex: String,
    /// `FastActorSessionV1::privileged_core_environment_hash`, which folds
    /// the ordered legal-action semantics of the current decision, so
    /// matching it is a match of the legal surface itself and not only of
    /// its width.
    pub core_environment_hash_u64_hex: String,
}

/// One corpus entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1CorpusDecisionV1 {
    pub coordinates: TtsS1ReplayCoordinatesV1,
    pub surface: TtsS1RecordedSurfaceV1,
    pub labels: TtsS1DecisionLabelsV1,
    /// Every stratum this decision belongs to, in the declared order.
    pub strata: Vec<TtsS1StratumV1>,
}

impl TtsS1CorpusDecisionV1 {
    pub fn is_in_stratum_v1(&self, stratum: TtsS1StratumV1) -> bool {
        self.strata.contains(&stratum)
    }
}

/// The loaded checkpoint's identity, copied off the same
/// [`ShadowCheckpointIdentityV1`] the CP7 scorer publishes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1CorpusCheckpointV1 {
    pub authority_kind: String,
    pub loaded_run_sha256: String,
    pub loaded_generation: u64,
    pub loaded_checkpoint_sha256: String,
    pub loaded_payload_sha256: String,
    pub loaded_train_state_sha256: String,
    pub model_parameter_sha256: String,
    pub net_architecture_identity: String,
    pub environment_trajectory_contract: String,
    pub sampler_identity: String,
    pub sampler_contract_sha256: String,
}

impl TtsS1CorpusCheckpointV1 {
    /// `pub(crate)`: `native_tts_s1_replay_v1` rebuilds this record from
    /// its own loaded identity and compares it against the corpus's, so
    /// the two must be built by one function, not two.
    pub(crate) fn from_identity_v1(
        identity: &ShadowCheckpointIdentityV1,
        architecture: &str,
    ) -> Self {
        Self {
            authority_kind: identity.authority_kind.clone(),
            loaded_run_sha256: identity.loaded_run_sha256.clone(),
            loaded_generation: identity.loaded_generation,
            loaded_checkpoint_sha256: identity.loaded_checkpoint_sha256.clone(),
            loaded_payload_sha256: identity.loaded_payload_sha256.clone(),
            loaded_train_state_sha256: identity.loaded_train_state_sha256.clone(),
            model_parameter_sha256: identity.model_parameter_sha256.clone(),
            net_architecture_identity: architecture.to_owned(),
            environment_trajectory_contract: identity.environment_trajectory_contract.to_owned(),
            sampler_identity: identity.sampler_identity.to_owned(),
            sampler_contract_sha256: identity.sampler_contract_sha256.to_owned(),
        }
    }
}

/// The pre-registered quotas, restated in the manifest so an auditor
/// reading only the file can check the corpus against them.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1CorpusQuotasV1 {
    pub target_decisions: u32,
    pub min_per_seat: u32,
    pub min_high_branching: u32,
    pub min_stack_interaction: u32,
    pub min_combat: u32,
    pub min_late_game: u32,
    pub high_branching_min_legal_actions: u32,
    pub stack_interaction_min_depth: u32,
    pub late_game_min_kernel_turn: u32,
    pub late_game_max_life: i32,
}

impl TtsS1CorpusQuotasV1 {
    pub const fn pre_registered_v1() -> Self {
        Self {
            target_decisions: TTS_S1_CORPUS_TARGET_DECISIONS_V1,
            min_per_seat: TTS_S1_CORPUS_MIN_PER_SEAT_V1,
            min_high_branching: TTS_S1_CORPUS_MIN_HIGH_BRANCHING_V1,
            min_stack_interaction: TTS_S1_CORPUS_MIN_STACK_INTERACTION_V1,
            min_combat: TTS_S1_CORPUS_MIN_COMBAT_V1,
            min_late_game: TTS_S1_CORPUS_MIN_LATE_GAME_V1,
            high_branching_min_legal_actions: TTS_S1_HIGH_BRANCHING_MIN_LEGAL_ACTIONS_V1,
            stack_interaction_min_depth: TTS_S1_STACK_INTERACTION_MIN_DEPTH_V1,
            late_game_min_kernel_turn: TTS_S1_LATE_GAME_MIN_KERNEL_TURN_V1,
            late_game_max_life: TTS_S1_LATE_GAME_MAX_LIFE_V1,
        }
    }
}

/// Everything the corpus digest covers. Split out from the envelope so
/// `corpus_sha256` can commit to the corpus without committing to itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1CorpusBodyV1 {
    pub engine_commit: String,
    pub checkpoint: TtsS1CorpusCheckpointV1,
    pub seed_block_id: u64,
    pub seed_block_seed: u64,
    pub episode_count: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub deck_ids: [String; 2],
    pub policy_sample_domain: String,
    pub selection_rule: String,
    pub quotas: TtsS1CorpusQuotasV1,
    /// How many decisions the self-play produced, before selection.
    pub candidate_count: u64,
    /// How many of `episode_count` episodes reached a natural terminal and
    /// therefore contributed candidates.
    pub natural_terminal_episode_count: u64,
    /// The selected corpus, in ascending (episode id, decision ordinal)
    /// order regardless of the order selection visited them in.
    pub decisions: Vec<TtsS1CorpusDecisionV1>,
}

/// The published manifest: a schema tag, the digest, and the body the
/// digest covers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TtsS1CorpusManifestV1 {
    pub schema: String,
    /// SHA-256 over the canonical JSON bytes of `body`, lower hex.
    pub corpus_sha256: String,
    pub body: TtsS1CorpusBodyV1,
}

impl TtsS1CorpusManifestV1 {
    /// Wraps a body, computing the digest. The only constructor, so a
    /// manifest carrying a wrong digest is unrepresentable.
    pub fn seal_v1(body: TtsS1CorpusBodyV1) -> Result<Self, TtsS1CorpusErrorV1> {
        let digest = corpus_body_digest_v1(&body)?;
        Ok(Self {
            schema: TTS_S1_CORPUS_SCHEMA_V1.to_owned(),
            corpus_sha256: lower_hex_sha256_v4(digest),
            body,
        })
    }

    /// Canonical bytes of the whole manifest, which is what gets
    /// published.
    pub fn canonical_bytes_v1(&self) -> Result<Vec<u8>, TtsS1CorpusErrorV1> {
        to_canonical_json_bytes_v1(self, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1CorpusErrorV1::CanonicalJson)
    }
}

/// SHA-256 over the canonical JSON bytes of a corpus body.
pub fn corpus_body_digest_v1(body: &TtsS1CorpusBodyV1) -> Result<[u8; 32], TtsS1CorpusErrorV1> {
    let bytes = to_canonical_json_bytes_v1(body, CanonicalJsonNullPolicyV1::Forbid)
        .map_err(|_| TtsS1CorpusErrorV1::CanonicalJson)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher.finalize().into())
}

/// Fail-closed error vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TtsS1CorpusErrorV1 {
    /// The seed block id is not in the model-guided allowlist.
    UnauthorizedSeedBlock,
    /// `episode_count` is zero or above [`TTS_S1_CORPUS_MAX_EPISODES_V1`].
    InvalidEpisodeCount,
    CheckpointLoad(String),
    /// The loaded model has no typed native net, so the tier replay could
    /// never run against it.
    ModelNotSearchCapable,
    EpisodeSchedule,
    SessionReset(String),
    Encode,
    Score,
    /// The model returned a width or a non-finite value the flat contract
    /// forbids.
    ScoreContract,
    Sample,
    Consume(String),
    /// Self-play produced fewer decisions than the quotas need.
    QuotaUnsatisfiable {
        stratum: TtsS1StratumV1,
        available: u32,
        required: u32,
    },
    /// The candidate pool was exhausted before the target size.
    CorpusTooSmall {
        selected: u32,
        required: u32,
    },
    /// A selected decision's episode action list is missing or shorter
    /// than its own ordinal. Structurally unreachable (the two come from
    /// the same harvest), so it is reported rather than assumed away.
    MissingEpisodeActions,
    CanonicalJson,
    /// A decoded manifest's `corpus_sha256` does not cover its own body,
    /// or its bytes were not canonical, or its schema is not this one.
    InvalidManifest,
    Publication(String),
}

impl TtsS1CorpusErrorV1 {
    pub fn code_v1(&self) -> &'static str {
        match self {
            Self::UnauthorizedSeedBlock => "tts_s1_corpus_unauthorized_seed_block",
            Self::InvalidEpisodeCount => "tts_s1_corpus_invalid_episode_count",
            Self::CheckpointLoad(_) => "tts_s1_corpus_checkpoint_load_failed",
            Self::ModelNotSearchCapable => "tts_s1_corpus_model_not_search_capable",
            Self::EpisodeSchedule => "tts_s1_corpus_episode_schedule_invalid",
            Self::SessionReset(_) => "tts_s1_corpus_session_reset_failed",
            Self::Encode => "tts_s1_corpus_decision_encoding_failed",
            Self::Score => "tts_s1_corpus_checkpoint_scoring_failed",
            Self::ScoreContract => "tts_s1_corpus_checkpoint_score_invalid",
            Self::Sample => "tts_s1_corpus_policy_sampling_failed",
            Self::Consume(_) => "tts_s1_corpus_action_consume_failed",
            Self::QuotaUnsatisfiable { .. } => "tts_s1_corpus_quota_unsatisfiable",
            Self::CorpusTooSmall { .. } => "tts_s1_corpus_too_small",
            Self::MissingEpisodeActions => "tts_s1_corpus_missing_episode_actions",
            Self::CanonicalJson => "tts_s1_corpus_canonical_json_failed",
            Self::InvalidManifest => "tts_s1_corpus_manifest_invalid",
            Self::Publication(_) => "tts_s1_corpus_publication_failed",
        }
    }
}

impl fmt::Display for TtsS1CorpusErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CheckpointLoad(detail)
            | Self::SessionReset(detail)
            | Self::Consume(detail)
            | Self::Publication(detail) => {
                write!(formatter, "{}: {detail}", self.code_v1())
            }
            Self::QuotaUnsatisfiable {
                stratum,
                available,
                required,
            } => write!(
                formatter,
                "{}: {stratum:?} has {available} candidates, needs {required}",
                self.code_v1()
            ),
            Self::CorpusTooSmall { selected, required } => write!(
                formatter,
                "{}: selected {selected}, needs {required}",
                self.code_v1()
            ),
            _ => formatter.write_str(self.code_v1()),
        }
    }
}

impl std::error::Error for TtsS1CorpusErrorV1 {}

/// Everything the launcher chooses. There is no default for any of it: a
/// corpus is a pre-registered artifact, so every input is stated.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsS1CorpusConfigV1 {
    pub authority: ShadowCheckpointAuthorityV1,
    /// Index into `MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1`.
    pub seed_block_id: usize,
    /// Episodes played, ids `0..episode_count`.
    pub episode_count: u64,
}

/// Per-decision policy-sampling seed.
///
/// Domain-separated from every other seed in the crate by
/// [`TTS_S1_CORPUS_POLICY_SAMPLE_DOMAIN_V1`], and symmetric in the acting
/// seat: the two seats are drawn by the identical rule, which is what
/// "both seats by the same checkpoint" requires.
pub fn corpus_policy_sample_seed_v1(
    base_seed: u64,
    episode_id: u64,
    decision: FastActorDecisionV1,
) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(TTS_S1_CORPUS_POLICY_SAMPLE_DOMAIN_V1.as_bytes());
    hasher.update(base_seed.to_be_bytes());
    hasher.update(episode_id.to_be_bytes());
    hasher.update(decision.physical_decision_id.to_be_bytes());
    hasher.update(decision.substep_index.to_be_bytes());
    hasher.update([player_seat_index_v1(decision.acting_player)]);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(first)
}

fn decision_kind_tag_v1(kind: FastActorDecisionKindV1) -> &'static str {
    match kind {
        FastActorDecisionKindV1::Surface => "surface",
        FastActorDecisionKindV1::AttackerInclusion => "attacker_inclusion",
        FastActorDecisionKindV1::BlockerInclusion => "blocker_inclusion",
    }
}

/// Reads the stratification labels off a live session positioned at
/// `decision`.
pub(crate) fn decision_labels_v1(
    session: &FastActorSessionV1,
    decision: FastActorDecisionV1,
) -> TtsS1DecisionLabelsV1 {
    use crate::state::Step;
    let state = session.kernel_search_state_v1();
    let life_p0 = state.players[0].life;
    let life_p1 = state.players[1].life;
    let kernel_turn = state.turn;
    let in_combat = matches!(
        state.step,
        Step::BeginCombat
            | Step::DeclareAttackers
            | Step::DeclareBlockers
            | Step::CombatDamage
            | Step::EndCombat
    );
    let combat_declaration_step =
        matches!(state.step, Step::DeclareAttackers | Step::DeclareBlockers);
    TtsS1DecisionLabelsV1 {
        seat: decision.acting_player,
        kernel_turn,
        phase_step: kernel_phase_step_name_v2(state.step).to_owned(),
        legal_action_count: decision.legal_action_count,
        stack_depth: u32::try_from(state.stack.len()).unwrap_or(u32::MAX),
        in_combat,
        combat_declaration_step,
        late_game: kernel_turn >= TTS_S1_LATE_GAME_MIN_KERNEL_TURN_V1
            || life_p0 <= TTS_S1_LATE_GAME_MAX_LIFE_V1
            || life_p1 <= TTS_S1_LATE_GAME_MAX_LIFE_V1,
        life_p0,
        life_p1,
    }
}

/// The strata a labelled decision belongs to, in the declared order.
pub fn strata_for_labels_v1(labels: &TtsS1DecisionLabelsV1) -> Vec<TtsS1StratumV1> {
    let mut strata = Vec::with_capacity(TtsS1StratumV1::DECLARED_ORDER_V1.len());
    for stratum in TtsS1StratumV1::DECLARED_ORDER_V1 {
        let member = match stratum {
            TtsS1StratumV1::SeatP0 => labels.seat == PlayerSeatV1::P0,
            TtsS1StratumV1::SeatP1 => labels.seat == PlayerSeatV1::P1,
            TtsS1StratumV1::HighBranching => {
                labels.legal_action_count >= TTS_S1_HIGH_BRANCHING_MIN_LEGAL_ACTIONS_V1
            }
            TtsS1StratumV1::StackInteraction => {
                labels.stack_depth >= TTS_S1_STACK_INTERACTION_MIN_DEPTH_V1
            }
            TtsS1StratumV1::Combat => labels.combat_declaration_step,
            TtsS1StratumV1::LateGame => labels.late_game,
        };
        if member {
            strata.push(stratum);
        }
    }
    strata
}

/// The pre-registered selection rule, as a pure function.
///
/// `candidates` must already be in ascending seed order (ascending base
/// seed, then episode id, then decision ordinal); this function never
/// re-sorts, so the caller's order IS the tie-break and a caller that
/// passed a different order would get a different, and wrong, corpus.
///
/// Phase 1, quota fill: walk [`TtsS1StratumV1::DECLARED_ORDER_V1`]. For
/// each stratum, count how many ALREADY-selected decisions belong to it
/// (a decision selected for an earlier stratum counts toward every
/// stratum it is in), then take unselected members in ascending order
/// until the quota is met. A stratum that cannot reach its quota is a
/// fail-closed error, never a smaller corpus.
///
/// Phase 2, fill to the target: cycle the same declared order, taking one
/// unselected member of each stratum per pass and skipping exhausted
/// strata, until the target is reached. Because every decision is in
/// exactly one of the two seat strata, the union of the six strata is the
/// whole candidate list, so this phase can only stop short when the whole
/// list is exhausted, which is again a fail-closed error.
///
/// Returns the selected indices into `candidates`, in ascending order.
pub fn select_tts_s1_corpus_v1(
    candidates: &[TtsS1CorpusDecisionV1],
) -> Result<Vec<usize>, TtsS1CorpusErrorV1> {
    let mut selected = vec![false; candidates.len()];
    let mut selected_count: u32 = 0;

    for stratum in TtsS1StratumV1::DECLARED_ORDER_V1 {
        let quota = stratum.quota_v1();
        let mut have: u32 = candidates
            .iter()
            .enumerate()
            .filter(|(index, candidate)| selected[*index] && candidate.is_in_stratum_v1(stratum))
            .count()
            .try_into()
            .unwrap_or(u32::MAX);
        for (index, candidate) in candidates.iter().enumerate() {
            if have >= quota {
                break;
            }
            if selected[index] || !candidate.is_in_stratum_v1(stratum) {
                continue;
            }
            selected[index] = true;
            selected_count += 1;
            have += 1;
        }
        if have < quota {
            return Err(TtsS1CorpusErrorV1::QuotaUnsatisfiable {
                stratum,
                available: have,
                required: quota,
            });
        }
    }

    // Phase 2: deterministic round-robin over the same declared order.
    let mut cursors = [0usize; 6];
    while selected_count < TTS_S1_CORPUS_TARGET_DECISIONS_V1 {
        let mut progressed = false;
        for (slot, stratum) in TtsS1StratumV1::DECLARED_ORDER_V1.into_iter().enumerate() {
            if selected_count >= TTS_S1_CORPUS_TARGET_DECISIONS_V1 {
                break;
            }
            let mut cursor = cursors[slot];
            while cursor < candidates.len()
                && (selected[cursor] || !candidates[cursor].is_in_stratum_v1(stratum))
            {
                cursor += 1;
            }
            cursors[slot] = cursor;
            if cursor < candidates.len() {
                selected[cursor] = true;
                selected_count += 1;
                cursors[slot] = cursor + 1;
                progressed = true;
            }
        }
        if !progressed {
            return Err(TtsS1CorpusErrorV1::CorpusTooSmall {
                selected: selected_count,
                required: TTS_S1_CORPUS_TARGET_DECISIONS_V1,
            });
        }
    }

    Ok(selected
        .iter()
        .enumerate()
        .filter_map(|(index, chosen)| chosen.then_some(index))
        .collect())
}

/// The narrow model seam both S1 tools need: the flat policy logits for
/// one live decision, and the typed native net the model-guided searcher
/// requires.
///
/// It exists for the same reason the scorer's own
/// `ShadowSearchCapableModelV1` does, and it is deliberately just as
/// narrow. Production always supplies
/// [`crate::native_checkpoint_inference_v1::NativeCheckpointInferenceV1`],
/// the checkpoint-manifest-backed handle `load_checkpoint_v1` returns; the
/// seam is what lets this crate's own tests drive the WHOLE S1 pipeline
/// (self-play, selection, reconstruction, the production search, the
/// report) against the in-memory `NativePolicyValueNetV1::runner_fixed_v1`
/// net that `native_checkpoint_shadow_stdio_v1`'s own search tests already
/// use, with no Store on disk. `pub(crate)`: it names two crate-private
/// types, and neither S1 module's public surface mentions it.
pub(crate) trait TtsS1DecisionScorerV1 {
    fn action_logits_v1(
        &self,
        decision: crate::flat_policy_v2::FlatScoringDecisionViewV2<'_>,
    ) -> Result<Vec<f32>, ()>;

    fn search_net_v1(&self) -> &crate::native_policy_value_net_v1::NativePolicyValueNetV1;
}

impl TtsS1DecisionScorerV1 for crate::native_checkpoint_inference_v1::NativeCheckpointInferenceV1 {
    fn action_logits_v1(
        &self,
        decision: crate::flat_policy_v2::FlatScoringDecisionViewV2<'_>,
    ) -> Result<Vec<f32>, ()> {
        Ok(self
            .score_decision_v1(decision)
            .map_err(|_| ())?
            .action_logits()
            .to_vec())
    }

    fn search_net_v1(&self) -> &crate::native_policy_value_net_v1::NativePolicyValueNetV1 {
        self.search_model_v1()
    }
}

/// One episode's self-play product.
pub(crate) struct EpisodeHarvestV1 {
    pub(crate) natural: bool,
    /// Every decision, in order, each carrying an EMPTY `action_sequence`.
    ///
    /// The prefixes are deferred rather than filled here because a
    /// candidate's prefix is as long as its ordinal: materializing one per
    /// candidate is quadratic in episode length and would be paid for
    /// every candidate, when at most 512 of them are ever selected. They
    /// are filled from [`Self::actions`] once selection is done, by
    /// [`EpisodeHarvestV1::action_prefix_v1`].
    pub(crate) decisions: Vec<TtsS1CorpusDecisionV1>,
    /// The whole episode's ordered flat-action indices, one per decision.
    pub(crate) actions: Vec<u32>,
}

/// The exact ordered action sequence that reaches `decision_ordinal`
/// within an episode whose whole action list is `actions`.
///
/// `None` when the ordinal is out of range, which is structurally
/// unreachable for a decision and an action list taken from the same
/// harvest, and is reported rather than assumed away.
pub(crate) fn action_prefix_v1(actions: &[u32], decision_ordinal: u64) -> Option<Vec<u32>> {
    let ordinal = usize::try_from(decision_ordinal).ok()?;
    actions.get(..ordinal).map(<[u32]>::to_vec)
}

impl EpisodeHarvestV1 {
    /// Fills every decision's prefix and returns them. Used where the
    /// whole episode is kept (this crate's own end-to-end test fixture),
    /// never on the corpus build path, which fills only what it selected.
    #[cfg(test)]
    pub(crate) fn into_decisions_with_action_sequences_v1(mut self) -> Vec<TtsS1CorpusDecisionV1> {
        for (ordinal, decision) in self.decisions.iter_mut().enumerate() {
            decision.coordinates.action_sequence =
                action_prefix_v1(&self.actions, ordinal as u64).unwrap_or_default();
        }
        self.decisions
    }
}

/// What one whole self-play sweep produced.
pub(crate) struct TtsS1CorpusSelectionV1 {
    pub(crate) decisions: Vec<TtsS1CorpusDecisionV1>,
    pub(crate) candidate_count: u64,
    pub(crate) natural_terminal_episode_count: u64,
}

/// Plays one seeded self-play episode with both seats on `scorer` and
/// records every decision.
pub(crate) fn harvest_episode_v1(
    scorer: &dyn TtsS1DecisionScorerV1,
    base_seed: u64,
    episode_id: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
) -> Result<EpisodeHarvestV1, TtsS1CorpusErrorV1> {
    use crate::async_flat_scored_rollout_v1::FlatScoredFamilyCore;
    use crate::async_flat_scored_rollout_v2::{FlatScoredFamilyV2, OwnedFlatScoringDecisionV2};
    use crate::flat_policy_v2::FlatDecisionEncoderV2;

    let schedule = native_trainer_episode_schedule_v1(base_seed, episode_id)
        .map_err(|_| TtsS1CorpusErrorV1::EpisodeSchedule)?;
    let deck_ids = [
        CANONICAL_RALLY_DECK_ID.to_owned(),
        CANONICAL_RALLY_DECK_ID.to_owned(),
    ];
    let mut session = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v2(
        episode_id,
        schedule.environment_seed,
        max_physical_decisions,
        max_policy_steps,
        deck_ids,
    )
    .map_err(|error| TtsS1CorpusErrorV1::SessionReset(format!("{:?}", error.code)))?;

    let mut encoder = FlatDecisionEncoderV2::default();
    let mut owned = OwnedFlatScoringDecisionV2::default();
    let mut actions: Vec<u32> = Vec::new();
    let mut decisions: Vec<TtsS1CorpusDecisionV1> = Vec::new();

    loop {
        let expected = match session.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                let natural = terminal.terminal_classification == TerminalClassificationV1::Natural;
                return Ok(EpisodeHarvestV1 {
                    natural,
                    decisions,
                    actions,
                });
            }
            FastActorResponseV1::Decision(expected) => expected,
        };

        let labels = decision_labels_v1(&session, expected);
        let strata = strata_for_labels_v1(&labels);
        let surface = TtsS1RecordedSurfaceV1 {
            policy_step: expected.step,
            environment_revision: expected.environment_revision,
            physical_decision_id: expected.physical_decision_id,
            substep_index: expected.substep_index,
            substep_count: expected.substep_count,
            acting_player: expected.acting_player,
            decision_kind: decision_kind_tag_v1(expected.decision_kind).to_owned(),
            legal_action_count: expected.legal_action_count,
            diagnostic_state_hash_u64_hex: format!("{:016x}", session.diagnostic_state_hash()),
            core_environment_hash_u64_hex: format!(
                "{:016x}",
                session.privileged_core_environment_hash()
            ),
        };

        let packet = FlatScoredFamilyV2::encode_packet(&session, expected, &mut encoder, owned)
            .map_err(|()| TtsS1CorpusErrorV1::Encode)?;
        let binding = FlatScoredFamilyV2::packet_binding(&packet);
        let logits = {
            let view = FlatScoredFamilyV2::packet_view(&packet);
            scorer
                .action_logits_v1(view)
                .map_err(|()| TtsS1CorpusErrorV1::Score)?
        };
        owned = FlatScoredFamilyV2::into_owned_packet(packet);
        if logits.len() != expected.legal_action_count as usize
            || logits.iter().any(|value| !value.is_finite())
        {
            return Err(TtsS1CorpusErrorV1::ScoreContract);
        }

        let seed = corpus_policy_sample_seed_v1(base_seed, episode_id, expected);
        let selected = FastCategoricalScratch::default()
            .sample(&logits, seed)
            .map_err(|_| TtsS1CorpusErrorV1::Sample)?;
        let selected = u32::try_from(selected).map_err(|_| TtsS1CorpusErrorV1::Sample)?;

        decisions.push(TtsS1CorpusDecisionV1 {
            coordinates: TtsS1ReplayCoordinatesV1 {
                episode_base_seed: base_seed,
                episode_id,
                environment_seed: schedule.environment_seed,
                decision_ordinal: actions.len() as u64,
                // Deferred; see `EpisodeHarvestV1::decisions`.
                action_sequence: Vec::new(),
            },
            surface,
            labels,
            strata,
        });
        actions.push(selected);

        session
            .consume_current_flat_action_slice_v2(binding.action_binding, selected)
            .map_err(|error| TtsS1CorpusErrorV1::Consume(format!("{:?}", error.code)))?;
    }
}

/// Plays `episode_count` episodes and applies the pre-registered
/// selection. Pure with respect to the filesystem: everything a Store
/// contributes is already in `scorer` and the two decision limits.
pub(crate) fn harvest_and_select_v1(
    scorer: &dyn TtsS1DecisionScorerV1,
    base_seed: u64,
    episode_count: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
) -> Result<TtsS1CorpusSelectionV1, TtsS1CorpusErrorV1> {
    let mut candidates: Vec<TtsS1CorpusDecisionV1> = Vec::new();
    // Kept per contributing episode so the selected decisions' replay
    // prefixes can be cut from them afterwards; see
    // `EpisodeHarvestV1::decisions` for why they are not cut per candidate.
    let mut episode_actions: Vec<(u64, Vec<u32>)> = Vec::new();
    let mut natural_terminal_episode_count: u64 = 0;
    for episode_id in 0..episode_count {
        let harvest = harvest_episode_v1(
            scorer,
            base_seed,
            episode_id,
            max_physical_decisions,
            max_policy_steps,
        )?;
        if harvest.natural {
            natural_terminal_episode_count += 1;
            candidates.extend(harvest.decisions);
            episode_actions.push((episode_id, harvest.actions));
        }
    }
    let candidate_count = candidates.len() as u64;
    let chosen = select_tts_s1_corpus_v1(&candidates)?;
    let mut decisions = Vec::with_capacity(chosen.len());
    for index in chosen {
        let mut decision = candidates[index].clone();
        let ordinal = decision.coordinates.decision_ordinal;
        let actions = episode_actions
            .iter()
            .find(|(episode_id, _)| *episode_id == decision.coordinates.episode_id)
            .map(|(_, actions)| actions)
            .ok_or(TtsS1CorpusErrorV1::MissingEpisodeActions)?;
        decision.coordinates.action_sequence =
            action_prefix_v1(actions, ordinal).ok_or(TtsS1CorpusErrorV1::MissingEpisodeActions)?;
        decisions.push(decision);
    }
    Ok(TtsS1CorpusSelectionV1 {
        decisions,
        candidate_count,
        natural_terminal_episode_count,
    })
}

/// Assembles the body a selection plus a checkpoint identity determine.
pub(crate) fn corpus_body_v1(
    checkpoint: TtsS1CorpusCheckpointV1,
    seed_block_id: usize,
    base_seed: u64,
    episode_count: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    selection: TtsS1CorpusSelectionV1,
) -> TtsS1CorpusBodyV1 {
    TtsS1CorpusBodyV1 {
        engine_commit: env!("MTG_KERNEL_BUILD_GIT_HEAD").to_owned(),
        checkpoint,
        seed_block_id: seed_block_id as u64,
        seed_block_seed: base_seed,
        episode_count,
        max_physical_decisions,
        max_policy_steps,
        deck_ids: [
            CANONICAL_RALLY_DECK_ID.to_owned(),
            CANONICAL_RALLY_DECK_ID.to_owned(),
        ],
        policy_sample_domain: TTS_S1_CORPUS_POLICY_SAMPLE_DOMAIN_V1.to_owned(),
        selection_rule: TTS_S1_CORPUS_SELECTION_RULE_V1.to_owned(),
        quotas: TtsS1CorpusQuotasV1::pre_registered_v1(),
        candidate_count: selection.candidate_count,
        natural_terminal_episode_count: selection.natural_terminal_episode_count,
        decisions: selection.decisions,
    }
}

/// Builds the frozen corpus: load the checkpoint, play the episodes, label
/// every decision, select under the pre-registered quotas, and seal.
pub fn build_tts_s1_corpus_v1(
    config: &TtsS1CorpusConfigV1,
) -> Result<TtsS1CorpusManifestV1, TtsS1CorpusErrorV1> {
    if config.episode_count == 0 || config.episode_count > TTS_S1_CORPUS_MAX_EPISODES_V1 {
        return Err(TtsS1CorpusErrorV1::InvalidEpisodeCount);
    }
    let base_seed = authorized_seed_block_v1(config.seed_block_id)
        .ok_or(TtsS1CorpusErrorV1::UnauthorizedSeedBlock)?;

    let loaded = load_checkpoint_v1(config.authority.clone())
        .map_err(|error| TtsS1CorpusErrorV1::CheckpointLoad(error.to_string()))?;
    let architecture = loaded
        .inference
        .search_model_v1()
        .architecture_identity_v1()
        .to_owned();

    let selection = harvest_and_select_v1(
        &loaded.inference,
        base_seed,
        config.episode_count,
        loaded.max_physical_decisions,
        loaded.max_policy_steps,
    )?;

    TtsS1CorpusManifestV1::seal_v1(corpus_body_v1(
        TtsS1CorpusCheckpointV1::from_identity_v1(&loaded.identity, &architecture),
        config.seed_block_id,
        base_seed,
        config.episode_count,
        loaded.max_physical_decisions,
        loaded.max_policy_steps,
        selection,
    ))
}

/// Decodes and re-proves a published corpus manifest.
///
/// Three independent checks, all fail-closed: the bytes must be exactly
/// canonical (a re-encode must reproduce them), the schema must be this
/// one, and `corpus_sha256` must cover the body it is published with.
pub fn decode_tts_s1_corpus_v1(bytes: &[u8]) -> Result<TtsS1CorpusManifestV1, TtsS1CorpusErrorV1> {
    let manifest: TtsS1CorpusManifestV1 =
        from_canonical_json_bytes_v1(bytes, CanonicalJsonNullPolicyV1::Forbid)
            .map_err(|_| TtsS1CorpusErrorV1::InvalidManifest)?;
    let reencoded = to_canonical_json_bytes_v1(&manifest, CanonicalJsonNullPolicyV1::Forbid)
        .map_err(|_| TtsS1CorpusErrorV1::CanonicalJson)?;
    if reencoded != bytes
        || manifest.schema != TTS_S1_CORPUS_SCHEMA_V1
        || manifest.corpus_sha256 != lower_hex_sha256_v4(corpus_body_digest_v1(&manifest.body)?)
    {
        return Err(TtsS1CorpusErrorV1::InvalidManifest);
    }
    Ok(manifest)
}

/// Publishes the manifest atomically and immutably.
///
/// Immutable, not replacing: the corpus is FROZEN, so publishing over an
/// existing one must be an error rather than a silent redefinition of the
/// population every later tier report claims to have measured.
pub fn publish_tts_s1_corpus_v1(
    manifest: &TtsS1CorpusManifestV1,
    path: &Path,
) -> Result<Vec<u8>, TtsS1CorpusErrorV1> {
    let bytes = manifest.canonical_bytes_v1()?;
    publish_canonical_document_v1(&bytes, path)?;
    Ok(bytes)
}

/// Shared atomic immutable publication used by both S1 tools.
pub(crate) fn publish_canonical_document_v1(
    bytes: &[u8],
    path: &Path,
) -> Result<(), TtsS1CorpusErrorV1> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let final_name = path.file_name().ok_or_else(|| {
        TtsS1CorpusErrorV1::Publication("output path has no file name".to_owned())
    })?;
    let stage_name = format!(
        "{}.stage-{}",
        final_name.to_string_lossy(),
        std::process::id()
    );
    let captured = capture_existing_publication_parent_v1(parent)
        .map_err(|error| TtsS1CorpusErrorV1::Publication(error.to_string()))?;
    let expectation = DurableFileExpectationV1::from_bytes(bytes)
        .map_err(|error| TtsS1CorpusErrorV1::Publication(error.to_string()))?;
    let staged = parent.join(&stage_name);
    if staged.exists() {
        // A leftover stage is stale by construction and is never the
        // artifact; only a failure to clear it is worth reporting.
        std::fs::remove_file(&staged)
            .map_err(|error| TtsS1CorpusErrorV1::Publication(error.to_string()))?;
    }
    let published =
        publish_immutable_file_by_move_v2(&captured, &stage_name, final_name, bytes, expectation);
    if published.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    published.map_err(|error| TtsS1CorpusErrorV1::Publication(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels_v1(
        seat: PlayerSeatV1,
        legal_action_count: u32,
        stack_depth: u32,
        combat: bool,
        late: bool,
    ) -> TtsS1DecisionLabelsV1 {
        TtsS1DecisionLabelsV1 {
            seat,
            kernel_turn: if late { 9 } else { 2 },
            phase_step: if combat {
                "DeclareAttackers".to_owned()
            } else {
                "Main1".to_owned()
            },
            legal_action_count,
            stack_depth,
            in_combat: combat,
            combat_declaration_step: combat,
            late_game: late,
            life_p0: 20,
            life_p1: 20,
        }
    }

    fn candidate_v1(
        episode_id: u64,
        decision_ordinal: u64,
        labels: TtsS1DecisionLabelsV1,
    ) -> TtsS1CorpusDecisionV1 {
        let strata = strata_for_labels_v1(&labels);
        TtsS1CorpusDecisionV1 {
            coordinates: TtsS1ReplayCoordinatesV1 {
                episode_base_seed: 3_101_001,
                episode_id,
                environment_seed: 7,
                decision_ordinal,
                action_sequence: vec![0; decision_ordinal as usize],
            },
            surface: TtsS1RecordedSurfaceV1 {
                policy_step: decision_ordinal,
                environment_revision: 1,
                physical_decision_id: decision_ordinal,
                substep_index: 0,
                substep_count: 1,
                acting_player: labels.seat,
                decision_kind: "surface".to_owned(),
                legal_action_count: labels.legal_action_count,
                diagnostic_state_hash_u64_hex: "0000000000000001".to_owned(),
                core_environment_hash_u64_hex: "0000000000000002".to_owned(),
            },
            labels,
            strata,
        }
    }

    /// A synthetic pool wide enough that every quota is reachable and the
    /// round-robin has to run: 900 candidates, cycling the label shapes.
    fn synthetic_pool_v1() -> Vec<TtsS1CorpusDecisionV1> {
        let mut pool = Vec::new();
        for index in 0..900u64 {
            let seat = if index % 2 == 0 {
                PlayerSeatV1::P0
            } else {
                PlayerSeatV1::P1
            };
            let labels = labels_v1(
                seat,
                if index % 3 == 0 { 8 } else { 2 },
                u32::from(index % 5 == 0),
                index % 7 == 0,
                index % 11 == 0,
            );
            pool.push(candidate_v1(index / 100, index % 100, labels));
        }
        pool
    }

    #[test]
    fn strata_membership_follows_the_pre_registered_thresholds_v1() {
        let high = labels_v1(PlayerSeatV1::P0, 6, 0, false, false);
        assert!(strata_for_labels_v1(&high).contains(&TtsS1StratumV1::HighBranching));
        let low = labels_v1(PlayerSeatV1::P0, 5, 0, false, false);
        assert!(!strata_for_labels_v1(&low).contains(&TtsS1StratumV1::HighBranching));

        let stacked = labels_v1(PlayerSeatV1::P1, 2, 1, false, false);
        let strata = strata_for_labels_v1(&stacked);
        assert!(strata.contains(&TtsS1StratumV1::StackInteraction));
        assert!(strata.contains(&TtsS1StratumV1::SeatP1));
        assert!(!strata.contains(&TtsS1StratumV1::SeatP0));

        // Late game by life alone, with an early kernel turn.
        let mut dying = labels_v1(PlayerSeatV1::P0, 2, 0, false, false);
        dying.kernel_turn = 3;
        dying.life_p1 = TTS_S1_LATE_GAME_MAX_LIFE_V1;
        dying.late_game = dying.kernel_turn >= TTS_S1_LATE_GAME_MIN_KERNEL_TURN_V1
            || dying.life_p0 <= TTS_S1_LATE_GAME_MAX_LIFE_V1
            || dying.life_p1 <= TTS_S1_LATE_GAME_MAX_LIFE_V1;
        assert!(strata_for_labels_v1(&dying).contains(&TtsS1StratumV1::LateGame));
    }

    #[test]
    fn selection_satisfies_every_quota_and_the_target_size_v1() {
        let pool = synthetic_pool_v1();
        let chosen = select_tts_s1_corpus_v1(&pool).expect("quotas are satisfiable");
        assert_eq!(chosen.len(), TTS_S1_CORPUS_TARGET_DECISIONS_V1 as usize);
        // Ascending, no duplicates.
        assert!(chosen.windows(2).all(|pair| pair[0] < pair[1]));
        for stratum in TtsS1StratumV1::DECLARED_ORDER_V1 {
            let have = chosen
                .iter()
                .filter(|index| pool[**index].is_in_stratum_v1(stratum))
                .count() as u32;
            assert!(
                have >= stratum.quota_v1(),
                "{stratum:?} got {have}, needs {}",
                stratum.quota_v1()
            );
        }
    }

    #[test]
    fn selection_is_a_pure_deterministic_function_of_candidate_order_v1() {
        let pool = synthetic_pool_v1();
        assert_eq!(
            select_tts_s1_corpus_v1(&pool).unwrap(),
            select_tts_s1_corpus_v1(&pool).unwrap()
        );
    }

    #[test]
    fn selection_fails_closed_on_an_unreachable_quota_v1() {
        // Every candidate is P0, so the P1 quota can never be met.
        let pool: Vec<_> = (0..900u64)
            .map(|index| {
                candidate_v1(
                    0,
                    index,
                    labels_v1(PlayerSeatV1::P0, 8, 1, index % 2 == 0, index % 3 == 0),
                )
            })
            .collect();
        let error = select_tts_s1_corpus_v1(&pool).unwrap_err();
        assert!(matches!(
            error,
            TtsS1CorpusErrorV1::QuotaUnsatisfiable {
                stratum: TtsS1StratumV1::SeatP1,
                available: 0,
                required: TTS_S1_CORPUS_MIN_PER_SEAT_V1,
            }
        ));
        assert_eq!(error.code_v1(), "tts_s1_corpus_quota_unsatisfiable");
    }

    #[test]
    fn selection_fails_closed_when_the_pool_cannot_reach_the_target_v1() {
        // Quotas are all reachable, but there are fewer than 512
        // candidates in total.
        let mut pool = Vec::new();
        for index in 0..400u64 {
            let seat = if index % 2 == 0 {
                PlayerSeatV1::P0
            } else {
                PlayerSeatV1::P1
            };
            pool.push(candidate_v1(0, index, labels_v1(seat, 8, 1, true, true)));
        }
        assert!(matches!(
            select_tts_s1_corpus_v1(&pool).unwrap_err(),
            TtsS1CorpusErrorV1::CorpusTooSmall {
                selected: 400,
                required: TTS_S1_CORPUS_TARGET_DECISIONS_V1,
            }
        ));
    }

    #[test]
    fn a_sealed_manifest_round_trips_and_rejects_a_tampered_digest_v1() {
        let pool = synthetic_pool_v1();
        let chosen = select_tts_s1_corpus_v1(&pool).unwrap();
        let body = TtsS1CorpusBodyV1 {
            engine_commit: "deadbeef".to_owned(),
            checkpoint: TtsS1CorpusCheckpointV1 {
                authority_kind: "test-only".to_owned(),
                loaded_run_sha256: "00".repeat(32),
                loaded_generation: 3,
                loaded_checkpoint_sha256: "11".repeat(32),
                loaded_payload_sha256: "22".repeat(32),
                loaded_train_state_sha256: "33".repeat(32),
                model_parameter_sha256: "44".repeat(32),
                net_architecture_identity: "kernel-policy-value-net-8".to_owned(),
                environment_trajectory_contract: "legacy_v1".to_owned(),
                sampler_identity: "f32-q8-expq63-hamilton-splitmix64-v1".to_owned(),
                sampler_contract_sha256: "55".repeat(32),
            },
            seed_block_id: 0,
            seed_block_seed: 3_101_001,
            episode_count: 9,
            max_physical_decisions: 1_024,
            max_policy_steps: 2_048,
            deck_ids: ["Rally".to_owned(), "Rally".to_owned()],
            policy_sample_domain: TTS_S1_CORPUS_POLICY_SAMPLE_DOMAIN_V1.to_owned(),
            selection_rule: TTS_S1_CORPUS_SELECTION_RULE_V1.to_owned(),
            quotas: TtsS1CorpusQuotasV1::pre_registered_v1(),
            candidate_count: pool.len() as u64,
            natural_terminal_episode_count: 9,
            decisions: chosen
                .into_iter()
                .map(|index| pool[index].clone())
                .collect(),
        };
        let manifest = TtsS1CorpusManifestV1::seal_v1(body).unwrap();
        let bytes = manifest.canonical_bytes_v1().unwrap();
        assert_eq!(decode_tts_s1_corpus_v1(&bytes).unwrap(), manifest);

        let mut tampered = manifest.clone();
        tampered.body.decisions[0].labels.kernel_turn += 1;
        let tampered_bytes = tampered.canonical_bytes_v1().unwrap();
        assert!(matches!(
            decode_tts_s1_corpus_v1(&tampered_bytes),
            Err(TtsS1CorpusErrorV1::InvalidManifest)
        ));
    }

    #[test]
    fn the_policy_sample_seed_is_domain_separated_and_seat_symmetric_v1() {
        let decision = FastActorDecisionV1 {
            episode_id: 4,
            step: 11,
            environment_revision: 1,
            physical_decision_id: 7,
            substep_index: 2,
            substep_count: 3,
            acting_player: PlayerSeatV1::P0,
            decision_kind: FastActorDecisionKindV1::Surface,
            legal_action_count: 5,
        };
        let p0 = corpus_policy_sample_seed_v1(3_101_001, 4, decision);
        let mut other_seat = decision;
        other_seat.acting_player = PlayerSeatV1::P1;
        let p1 = corpus_policy_sample_seed_v1(3_101_001, 4, other_seat);
        assert_ne!(p0, p1, "the two seats must not share a draw");
        assert_eq!(p0, corpus_policy_sample_seed_v1(3_101_001, 4, decision));
        assert_ne!(p0, corpus_policy_sample_seed_v1(3_102_001, 4, decision));
        assert_ne!(p0, corpus_policy_sample_seed_v1(3_101_001, 5, decision));
        let mut other_substep = decision;
        other_substep.substep_index = 3;
        assert_ne!(
            p0,
            corpus_policy_sample_seed_v1(3_101_001, 4, other_substep)
        );
    }
}
