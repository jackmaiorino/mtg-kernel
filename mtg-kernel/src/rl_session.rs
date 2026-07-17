//! Interactive RL session protocol for deck-identified kernel environments.
//!
//! This module owns the reset/step state machine used by both the JSONL
//! process wrapper and the batch rollout recorder, so action validation and
//! terminal classification cannot drift between interactive and offline use.
//! Schema v5 carries ordered physical-seat deck identity on the wire. Exact
//! canonical `Burn` and `Rally` ids may be combined in any ordered pair; every
//! other id fails before an active session is created or replaced.

use crate::card_def::KERNEL_CARDDB_HASH;
use crate::engine::Decision;
use crate::ids::PlayerId;
use crate::policy_surface_v5::{
    PolicyDecisionV5, PolicySurfaceV5, POLICY_ENVIRONMENT_HASH_ALGORITHM, POLICY_SURFACE_VERSION,
};
use crate::rl::{
    build_deck_pair_state, legal_action_candidates_v5, observe_policy_v5, parse_strict_json_value,
    EpisodeTerminalSummaryV1, LegalActionV5, ObservationV5, PlayerSeatV1,
    PolicyLegalActionCandidateV5, RlContractError, TerminalClassificationV1, TerminalOutcomeV1,
    TerminalSafeCodeV2,
};
use crate::runtime_decks::{runtime_deck_by_id, RuntimeDeckDefinition};
use crate::surface_v2::{SurfaceDecision, H2_PREDICATE_VERSION};
use crate::KERNEL_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub const RL_SESSION_SCHEMA_VERSION: u32 = 5;
pub const RL_SESSION_PROTOCOL_VERSION: u32 = 5;
pub const RL_SESSION_PROTOCOL_NAME: &str = "kernel_rl_jsonl";
pub const CANONICAL_BURN_DECK_ID: &str = "Burn";
pub const CANONICAL_RALLY_DECK_ID: &str = "Rally";

pub type SessionDeckIdsV1 = [String; 2];
pub type SessionDeckHashesV1 = [u64; 2];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlSessionProvenanceV1 {
    pub protocol: String,
    pub protocol_version: u32,
    pub schema_version: u32,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
}

impl RlSessionProvenanceV1 {
    pub fn current() -> Self {
        RlSessionProvenanceV1 {
            protocol: RL_SESSION_PROTOCOL_NAME.to_string(),
            protocol_version: RL_SESSION_PROTOCOL_VERSION,
            schema_version: RL_SESSION_SCHEMA_VERSION,
            kernel_version: KERNEL_VERSION.to_string(),
            surface_version: H2_PREDICATE_VERSION,
            policy_surface_version: POLICY_SURFACE_VERSION,
            card_db_hash: KERNEL_CARDDB_HASH,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlSessionDecisionV1 {
    pub schema_version: u32,
    pub deck_ids: SessionDeckIdsV1,
    pub deck_hashes: SessionDeckHashesV1,
    pub episode_id: u64,
    pub step: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub acting_player: PlayerSeatV1,
    pub observation: Box<ObservationV5>,
    pub legal_actions: Vec<LegalActionV5>,
    pub reward: [i32; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlSessionTerminalV1 {
    pub schema_version: u32,
    pub deck_ids: SessionDeckIdsV1,
    pub deck_hashes: SessionDeckHashesV1,
    pub episode_id: u64,
    pub terminal_outcome: TerminalOutcomeV1,
    pub terminal_classification: TerminalClassificationV1,
    pub terminal_code: TerminalSafeCodeV2,
    pub winner: Option<PlayerSeatV1>,
    pub terminal_reward: [i32; 2],
    pub terminal_reason: String,
    pub policy_step_count: u64,
    pub physical_decision_count: u64,
}

impl From<RlSessionTerminalV1> for EpisodeTerminalSummaryV1 {
    fn from(value: RlSessionTerminalV1) -> Self {
        EpisodeTerminalSummaryV1 {
            episode_id: value.episode_id,
            outcome: value.terminal_outcome,
            classification: value.terminal_classification,
            winner: value.winner,
            terminal_reward: value.terminal_reward,
            terminal_reason: value.terminal_reason,
            policy_step_count: value.policy_step_count,
            physical_decision_count: value.physical_decision_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RlSessionResponseV1 {
    Decision(RlSessionDecisionV1),
    Terminal(RlSessionTerminalV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RlSessionErrorCode {
    EpisodeAlreadyTerminal,
    EpisodeIdMismatch,
    ExpectedStepMismatch,
    SelectedIndexOutOfRange,
    SelectedActionIdMismatch,
    SelectedActionIdUnknown,
    StaleEnvironmentBinding,
    UnsupportedDeck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlSessionError {
    pub code: RlSessionErrorCode,
    pub message: String,
}

impl fmt::Display for RlSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for RlSessionError {}

impl From<RlSessionError> for RlContractError {
    fn from(value: RlSessionError) -> Self {
        RlContractError(value.to_string())
    }
}

#[derive(Debug, Clone)]
struct CurrentDecisionV1 {
    actor: PlayerId,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    observation: ObservationV5,
    candidates: Vec<PolicyLegalActionCandidateV5>,
    environment_hash: u64,
}

#[derive(Clone)]
pub struct RlEpisodeSessionV1 {
    deck_ids: SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    state: crate::state::GameState,
    surface: PolicySurfaceV5,
    policy_step_count: u64,
    physical_decision_count: u64,
    current: Option<CurrentDecisionV1>,
    terminal: Option<RlSessionTerminalV1>,
}

#[derive(Clone)]
pub struct RlEpisodeSessionSnapshotV5(RlEpisodeSessionV1);

impl RlEpisodeSessionV1 {
    pub fn reset(episode_id: u64, env_seed: u64, max_physical_decisions: u64) -> Self {
        let max_policy_steps = max_physical_decisions.saturating_mul(128).max(1);
        Self::reset_with_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
        )
    }

    pub fn reset_with_limits(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> Self {
        Self::reset_with_decks_and_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            canonical_burn_mirror_deck_ids(),
        )
        .expect("the built-in Burn/Burn deck pair is supported")
    }

    pub fn reset_with_decks(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        deck_ids: SessionDeckIdsV1,
    ) -> Result<Self, RlSessionError> {
        let max_policy_steps = max_physical_decisions.saturating_mul(128).max(1);
        Self::reset_with_decks_and_limits(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
        )
    }

    pub fn reset_with_decks_and_limits(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
    ) -> Result<Self, RlSessionError> {
        let resolved_decks = resolve_runtime_decks(&deck_ids)?;
        let deck_hashes = resolved_decks.map(|deck| deck.runtime_deck_hash);
        let state = build_deck_pair_state(
            env_seed,
            resolved_decks[0].card_ids,
            resolved_decks[1].card_ids,
        )
        .map_err(|_| {
            session_error(
                RlSessionErrorCode::UnsupportedDeck,
                "runtime deck catalog failed full-support preflight",
            )
        })?;
        let mut session = RlEpisodeSessionV1 {
            deck_ids,
            deck_hashes,
            episode_id,
            max_physical_decisions,
            max_policy_steps,
            state,
            surface: PolicySurfaceV5::new(),
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            terminal: None,
        };
        session.advance_to_decision_or_terminal();
        Ok(session)
    }

    pub fn current_response(&self) -> RlSessionResponseV1 {
        if let Some(terminal) = &self.terminal {
            return RlSessionResponseV1::Terminal(terminal.clone());
        }
        let current = self
            .current
            .as_ref()
            .expect("session has either a current decision or terminal");
        RlSessionResponseV1::Decision(RlSessionDecisionV1 {
            schema_version: RL_SESSION_SCHEMA_VERSION,
            deck_ids: self.deck_ids.clone(),
            deck_hashes: self.deck_hashes,
            episode_id: self.episode_id,
            step: self.policy_step_count,
            physical_decision_id: current.physical_decision_id,
            substep_index: current.substep_index,
            substep_count: current.substep_count,
            acting_player: current.actor.into(),
            observation: Box::new(current.observation.clone()),
            legal_actions: current
                .candidates
                .iter()
                .map(|c| c.record.clone())
                .collect(),
            reward: [0, 0],
        })
    }

    pub fn policy_step_count(&self) -> u64 {
        self.policy_step_count
    }

    pub fn physical_decision_count(&self) -> u64 {
        self.physical_decision_count
    }

    pub fn diagnostic_state_hash(&self) -> u64 {
        self.state.diagnostic_state_hash()
    }

    pub fn privileged_environment_hash(&self) -> u64 {
        self.compute_environment_hash(self.current.as_ref())
            .expect("session environment serializes")
    }

    pub fn snapshot_v5(&self) -> RlEpisodeSessionSnapshotV5 {
        RlEpisodeSessionSnapshotV5(self.clone())
    }

    pub fn restore_v5(&mut self, snapshot: &RlEpisodeSessionSnapshotV5) {
        *self = snapshot.0.clone();
    }

    pub fn step(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        selected_action_id: &str,
    ) -> Result<RlSessionResponseV1, RlSessionError> {
        if episode_id != self.episode_id {
            return Err(session_error(
                RlSessionErrorCode::EpisodeIdMismatch,
                "step request episode_id does not match the active episode",
            ));
        }
        if expected_step != self.policy_step_count {
            return Err(session_error(
                RlSessionErrorCode::ExpectedStepMismatch,
                "step request expected_step does not match the active decision step",
            ));
        }
        if self.terminal.is_some() {
            return Err(session_error(
                RlSessionErrorCode::EpisodeAlreadyTerminal,
                "episode is already terminal; reset before stepping again",
            ));
        }
        let current = self
            .current
            .as_ref()
            .expect("nonterminal session has current decision");
        let rebound = self.compute_environment_hash(Some(current)).map_err(|_| {
            session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "active decision integrity validation failed",
            )
        })?;
        if rebound != current.environment_hash {
            return Err(session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "active decision no longer matches its privileged environment binding",
            ));
        }
        let selected_index_usize = selected_index as usize;
        let Some(selected) = current.candidates.get(selected_index_usize) else {
            return Err(session_error(
                RlSessionErrorCode::SelectedIndexOutOfRange,
                "selected_index is outside the current legal action list",
            ));
        };
        if selected.record.stable_id != selected_action_id {
            let code = if current
                .candidates
                .iter()
                .any(|candidate| candidate.record.stable_id == selected_action_id)
            {
                RlSessionErrorCode::SelectedActionIdMismatch
            } else {
                RlSessionErrorCode::SelectedActionIdUnknown
            };
            return Err(session_error(
                code,
                "selected_index and selected_action_id do not identify the same current action",
            ));
        }
        let policy_action = selected.policy_action.clone();
        let completes_physical = current.substep_index + 1 == current.substep_count;
        self.surface
            .apply(&mut self.state, policy_action)
            .map_err(|_| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "selected action no longer matches the active policy environment",
                )
            })?;
        self.current = None;
        self.policy_step_count += 1;
        if completes_physical {
            self.physical_decision_count += 1;
        }
        self.advance_to_decision_or_terminal();
        Ok(self.current_response())
    }

    fn advance_to_decision_or_terminal(&mut self) {
        self.current = None;
        let surfaced = match self.surface.next_decision(&mut self.state) {
            Ok(decision) => decision,
            Err(_) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    "fail_closed:policy_surface_environment".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        match &surfaced {
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::GameOver { winner })) => {
                self.terminal = Some(terminal_from_winner(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    *winner,
                    "game_over".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::Halted {
                mechanic,
                source,
            })) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    format!("engine_halted:{mechanic:?}:source:{}", source.0),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
            _ => {}
        }
        let (substep_index, substep_count) = surfaced.substep();
        if substep_index == 0 && self.physical_decision_count >= self.max_physical_decisions {
            let _ = self.surface.discard_unanswered_scan();
            self.terminal = Some(truncated_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                format!(
                    "physical_decision_cap_reached:{}",
                    self.max_physical_decisions
                ),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let remaining_group_steps = u64::from(substep_count - substep_index);
        if self.policy_step_count.saturating_add(remaining_group_steps) > self.max_policy_steps {
            if substep_index == 0 {
                let _ = self.surface.discard_unanswered_scan();
            }
            self.terminal = Some(truncated_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                format!("policy_step_cap_reached:{}", self.max_policy_steps),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let Some(actor) = surfaced.actor(&self.state) else {
            self.terminal = Some(halted_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                "fail_closed:nonterminal decision without acting player".to_string(),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        };
        let observation = match observe_policy_v5(
            &self.state,
            &self.surface,
            actor,
            self.policy_step_count,
            self.physical_decision_count,
            substep_index,
            substep_count,
        ) {
            Ok(observation) => observation,
            Err(err) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    format!("fail_closed:observation:{err}"),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        let candidates = match legal_action_candidates_v5(&surfaced, &self.state) {
            Ok(candidates) => candidates,
            Err(err) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    format!("fail_closed:{err}"),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        if candidates.is_empty() {
            self.terminal = Some(halted_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                "fail_closed:nonterminal decision produced zero legal actions".to_string(),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let mut current = CurrentDecisionV1 {
            actor,
            physical_decision_id: self.physical_decision_count,
            substep_index,
            substep_count,
            observation,
            candidates,
            environment_hash: 0,
        };
        current.environment_hash = match self.compute_environment_hash(Some(&current)) {
            Ok(hash) => hash,
            Err(_) => {
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    "fail_closed:session_integrity".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return;
            }
        };
        self.current = Some(current);
    }

    fn compute_environment_hash(&self, current: Option<&CurrentDecisionV1>) -> Result<u64, String> {
        #[derive(Serialize)]
        struct PolicyEnvironmentEnvelopeV1 {
            schema_version: u32,
            hash_algorithm: &'static str,
            diagnostic_state_hash_algorithm: &'static str,
            diagnostic_state_hash: u64,
            harness_surface_context: crate::surface_v2::HarnessSurfacePublicContextV2,
            policy_surface_context: crate::policy_surface_v5::PolicySurfaceContextIdsV5,
            policy_step_count: u64,
            physical_decision_count: u64,
            current_actor: Option<PlayerId>,
            physical_decision_id: Option<u64>,
            substep_index: Option<u32>,
            substep_count: Option<u32>,
            observation_projection_hash: Option<u64>,
            legal_action_ids: Vec<String>,
        }

        let envelope = PolicyEnvironmentEnvelopeV1 {
            schema_version: 1,
            hash_algorithm: POLICY_ENVIRONMENT_HASH_ALGORITHM,
            diagnostic_state_hash_algorithm: crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
            diagnostic_state_hash: self.state.diagnostic_state_hash(),
            harness_surface_context: self.surface.harness_public_context(),
            policy_surface_context: self.surface.privileged_scan_context()?,
            policy_step_count: self.policy_step_count,
            physical_decision_count: self.physical_decision_count,
            current_actor: current.map(|decision| decision.actor),
            physical_decision_id: current.map(|decision| decision.physical_decision_id),
            substep_index: current.map(|decision| decision.substep_index),
            substep_count: current.map(|decision| decision.substep_count),
            observation_projection_hash: current
                .map(|decision| decision.observation.visible_projection_hash),
            legal_action_ids: current
                .map(|decision| {
                    decision
                        .candidates
                        .iter()
                        .map(|candidate| candidate.record.stable_id.clone())
                        .collect()
                })
                .unwrap_or_default(),
        };
        let bytes = serde_json::to_vec(&envelope).map_err(|err| err.to_string())?;
        Ok(fnv1a64(&bytes))
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "request_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum KernelRlRequestV1 {
    Reset {
        schema_version: u32,
        request_id: String,
        deck_ids: SessionDeckIdsV1,
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    },
    Step {
        schema_version: u32,
        request_id: String,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        selected_action_id: String,
    },
}

impl KernelRlRequestV1 {
    fn request_id(&self) -> &str {
        match self {
            KernelRlRequestV1::Reset { request_id, .. }
            | KernelRlRequestV1::Step { request_id, .. } => request_id,
        }
    }

    fn schema_version(&self) -> u32 {
        match self {
            KernelRlRequestV1::Reset { schema_version, .. }
            | KernelRlRequestV1::Step { schema_version, .. } => *schema_version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelRlErrorV1 {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "response_type", rename_all = "snake_case")]
pub enum KernelRlResponseV1 {
    Decision {
        schema_version: u32,
        request_id: String,
        provenance: RlSessionProvenanceV1,
        deck_ids: SessionDeckIdsV1,
        deck_hashes: SessionDeckHashesV1,
        episode_id: u64,
        step: u64,
        physical_decision_id: u64,
        substep_index: u32,
        substep_count: u32,
        acting_player: PlayerSeatV1,
        observation: Box<ObservationV5>,
        legal_actions: Vec<LegalActionV5>,
        reward: [i32; 2],
    },
    Terminal {
        schema_version: u32,
        request_id: String,
        provenance: RlSessionProvenanceV1,
        deck_ids: SessionDeckIdsV1,
        deck_hashes: SessionDeckHashesV1,
        episode_id: u64,
        terminal_outcome: TerminalOutcomeV1,
        terminal_classification: TerminalClassificationV1,
        terminal_code: TerminalSafeCodeV2,
        winner: Option<PlayerSeatV1>,
        terminal_reward: [i32; 2],
        terminal_reason: String,
        policy_step_count: u64,
        physical_decision_count: u64,
    },
    Error {
        schema_version: u32,
        request_id: Option<String>,
        error: KernelRlErrorV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedProtocolExchangeV1 {
    // Request ids are process-unique except for one immediate identical retry.
    // The cache is deliberately one entry so stale-step safety comes from the
    // episode_id/expected_step preconditions, not an unbounded replay table.
    request_id: String,
    request: KernelRlRequestV1,
    response_line: String,
}

#[derive(Default)]
pub struct KernelRlJsonlServerV1 {
    session: Option<RlEpisodeSessionV1>,
    last_exchange: Option<CachedProtocolExchangeV1>,
}

impl KernelRlJsonlServerV1 {
    pub fn new() -> Self {
        KernelRlJsonlServerV1::default()
    }

    pub fn handle_line(&mut self, line: &str) -> String {
        let value = match parse_strict_json_value(line) {
            Ok(value) => value,
            Err(_) => {
                if serde_json::from_str::<serde::de::IgnoredAny>(line).is_ok() {
                    return serialize_response(error_response(
                        None,
                        "malformed_request",
                        "request does not match the v5 protocol schema",
                    ));
                }
                return serialize_response(error_response(
                    None,
                    "malformed_json",
                    "request line is not valid JSON",
                ));
            }
        };
        let request_id = request_id_from_value(&value);
        let request = match serde_json::from_value::<KernelRlRequestV1>(value) {
            Ok(request) => request,
            Err(_) => {
                return serialize_response(error_response(
                    request_id,
                    "malformed_request",
                    "request does not match the v5 protocol schema",
                ));
            }
        };
        if let Some(cached) = &self.last_exchange {
            if cached.request_id == request.request_id() {
                if cached.request == request {
                    return cached.response_line.clone();
                }
                return serialize_response(error_response(
                    Some(request.request_id().to_string()),
                    "request_id_reuse_mismatch",
                    "request_id was reused for a different immediate request payload",
                ));
            }
        }
        let should_cache = request.schema_version() == RL_SESSION_SCHEMA_VERSION;
        let request_for_cache = request.clone();
        let response = self.handle_request(request);
        let response_line = serialize_response(response);
        if should_cache {
            self.last_exchange = Some(CachedProtocolExchangeV1 {
                request_id: request_for_cache.request_id().to_string(),
                request: request_for_cache,
                response_line: response_line.clone(),
            });
        }
        response_line
    }

    fn handle_request(&mut self, request: KernelRlRequestV1) -> KernelRlResponseV1 {
        match request {
            KernelRlRequestV1::Reset {
                schema_version,
                request_id,
                deck_ids,
                episode_id,
                env_seed,
                max_physical_decisions,
                max_policy_steps,
            } => {
                if schema_version != RL_SESSION_SCHEMA_VERSION {
                    return error_response(
                        Some(request_id),
                        "schema_version_mismatch",
                        "unsupported request schema_version",
                    );
                }
                let session = match RlEpisodeSessionV1::reset_with_decks_and_limits(
                    episode_id,
                    env_seed,
                    max_physical_decisions,
                    max_policy_steps,
                    deck_ids,
                ) {
                    Ok(session) => session,
                    Err(err) => {
                        return error_response(
                            Some(request_id),
                            session_error_code(&err.code),
                            &err.message,
                        );
                    }
                };
                let response = session_response_to_protocol(request_id, session.current_response());
                self.session = Some(session);
                response
            }
            KernelRlRequestV1::Step {
                schema_version,
                request_id,
                episode_id,
                expected_step,
                selected_index,
                selected_action_id,
            } => {
                if schema_version != RL_SESSION_SCHEMA_VERSION {
                    return error_response(
                        Some(request_id),
                        "schema_version_mismatch",
                        "unsupported request schema_version",
                    );
                }
                let Some(session) = self.session.as_mut() else {
                    return error_response(
                        Some(request_id),
                        "step_before_reset",
                        "step request received before reset",
                    );
                };
                match session.step(
                    episode_id,
                    expected_step,
                    selected_index,
                    &selected_action_id,
                ) {
                    Ok(response) => session_response_to_protocol(request_id, response),
                    Err(err) => error_response(
                        Some(request_id),
                        session_error_code(&err.code),
                        &err.message,
                    ),
                }
            }
        }
    }
}

fn session_response_to_protocol(
    request_id: String,
    response: RlSessionResponseV1,
) -> KernelRlResponseV1 {
    match response {
        RlSessionResponseV1::Decision(decision) => KernelRlResponseV1::Decision {
            schema_version: RL_SESSION_SCHEMA_VERSION,
            request_id,
            provenance: RlSessionProvenanceV1::current(),
            deck_ids: decision.deck_ids,
            deck_hashes: decision.deck_hashes,
            episode_id: decision.episode_id,
            step: decision.step,
            physical_decision_id: decision.physical_decision_id,
            substep_index: decision.substep_index,
            substep_count: decision.substep_count,
            acting_player: decision.acting_player,
            observation: decision.observation,
            legal_actions: decision.legal_actions,
            reward: decision.reward,
        },
        RlSessionResponseV1::Terminal(terminal) => KernelRlResponseV1::Terminal {
            schema_version: RL_SESSION_SCHEMA_VERSION,
            request_id,
            provenance: RlSessionProvenanceV1::current(),
            deck_ids: terminal.deck_ids,
            deck_hashes: terminal.deck_hashes,
            episode_id: terminal.episode_id,
            terminal_outcome: terminal.terminal_outcome,
            terminal_classification: terminal.terminal_classification,
            terminal_code: terminal.terminal_code,
            winner: terminal.winner,
            terminal_reward: terminal.terminal_reward,
            terminal_reason: terminal.terminal_reason,
            policy_step_count: terminal.policy_step_count,
            physical_decision_count: terminal.physical_decision_count,
        },
    }
}

fn serialize_response(response: KernelRlResponseV1) -> String {
    serde_json::to_string(&response).expect("protocol response serializes")
}

fn request_id_from_value(value: &Value) -> Option<String> {
    value
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn error_response(request_id: Option<String>, code: &str, message: &str) -> KernelRlResponseV1 {
    KernelRlResponseV1::Error {
        schema_version: RL_SESSION_SCHEMA_VERSION,
        request_id,
        error: KernelRlErrorV1 {
            code: code.to_string(),
            message: message.to_string(),
        },
    }
}

fn session_error(code: RlSessionErrorCode, message: &str) -> RlSessionError {
    RlSessionError {
        code,
        message: message.to_string(),
    }
}

fn session_error_code(code: &RlSessionErrorCode) -> &'static str {
    match code {
        RlSessionErrorCode::EpisodeAlreadyTerminal => "episode_already_terminal",
        RlSessionErrorCode::EpisodeIdMismatch => "episode_id_mismatch",
        RlSessionErrorCode::ExpectedStepMismatch => "expected_step_mismatch",
        RlSessionErrorCode::SelectedIndexOutOfRange => "selected_index_out_of_range",
        RlSessionErrorCode::SelectedActionIdMismatch => "selected_action_id_mismatch",
        RlSessionErrorCode::SelectedActionIdUnknown => "selected_action_id_unknown",
        RlSessionErrorCode::StaleEnvironmentBinding => "stale_environment_binding",
        RlSessionErrorCode::UnsupportedDeck => "unsupported_deck",
    }
}

fn canonical_burn_mirror_deck_ids() -> SessionDeckIdsV1 {
    [
        CANONICAL_BURN_DECK_ID.to_string(),
        CANONICAL_BURN_DECK_ID.to_string(),
    ]
}

fn resolve_runtime_decks(
    deck_ids: &SessionDeckIdsV1,
) -> Result<[&'static RuntimeDeckDefinition; 2], RlSessionError> {
    let mut resolved = [None, None];
    for (seat, deck_id) in deck_ids.iter().enumerate() {
        let Some(deck) = runtime_deck_by_id(deck_id) else {
            return Err(session_error(
                RlSessionErrorCode::UnsupportedDeck,
                &format!(
                    "unsupported deck_id for seat {seat}; supported exact canonical ids are {CANONICAL_BURN_DECK_ID:?} and {CANONICAL_RALLY_DECK_ID:?}"
                ),
            ));
        };
        resolved[seat] = Some(deck);
    }
    Ok([
        resolved[0].expect("both deck seats resolve"),
        resolved[1].expect("both deck seats resolve"),
    ])
}

fn terminal_from_winner(
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    winner: Option<PlayerId>,
    terminal_reason: String,
    policy_step_count: u64,
    physical_decision_count: u64,
) -> RlSessionTerminalV1 {
    let (terminal_outcome, terminal_reward) = match winner {
        Some(PlayerId::P0) => (TerminalOutcomeV1::P0Win, [1, -1]),
        Some(PlayerId::P1) => (TerminalOutcomeV1::P1Win, [-1, 1]),
        None => (TerminalOutcomeV1::Draw, [0, 0]),
        Some(other) => panic!("unsupported winner player id {}", other.0),
    };
    RlSessionTerminalV1 {
        schema_version: RL_SESSION_SCHEMA_VERSION,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        episode_id,
        terminal_outcome,
        terminal_classification: TerminalClassificationV1::Natural,
        terminal_code: TerminalSafeCodeV2::NaturalGameOver,
        winner: winner.map(Into::into),
        terminal_reward,
        terminal_reason,
        policy_step_count,
        physical_decision_count,
    }
}

fn halted_terminal(
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    terminal_reason: String,
    policy_step_count: u64,
    physical_decision_count: u64,
) -> RlSessionTerminalV1 {
    RlSessionTerminalV1 {
        schema_version: RL_SESSION_SCHEMA_VERSION,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        episode_id,
        terminal_outcome: TerminalOutcomeV1::Halted,
        terminal_classification: TerminalClassificationV1::Halted,
        terminal_code: TerminalSafeCodeV2::FailClosed,
        winner: None,
        terminal_reward: [0, 0],
        terminal_reason,
        policy_step_count,
        physical_decision_count,
    }
}

fn truncated_terminal(
    deck_ids: &SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    terminal_reason: String,
    policy_step_count: u64,
    physical_decision_count: u64,
) -> RlSessionTerminalV1 {
    RlSessionTerminalV1 {
        schema_version: RL_SESSION_SCHEMA_VERSION,
        deck_ids: deck_ids.clone(),
        deck_hashes,
        episode_id,
        terminal_outcome: TerminalOutcomeV1::Truncated,
        terminal_classification: TerminalClassificationV1::Truncated,
        terminal_code: TerminalSafeCodeV2::DecisionCap,
        winner: None,
        terminal_reward: [0, 0],
        terminal_reason,
        policy_step_count,
        physical_decision_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::card_id_by_name;
    use crate::rl::card_name;
    use crate::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};

    fn attacker_state(count: usize) -> GameState {
        let mut state = GameState::new_from_libraries(&[], &[], card_name, 91);
        state.step = Step::DeclareAttackers;
        state.active_player = PlayerId::P0;
        state.priority_player = PlayerId::P0;
        state.engine.combat.attackers_declared = false;
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        for _ in 0..count {
            let id = state.objects.push(GameObject {
                card_def,
                name: "Voldaren Epicure".to_string(),
                owner: PlayerId::P0,
                controller: PlayerId::P0,
                zone: Zone::Battlefield,
                tapped: false,
                summoning_sick: false,
                damage: 0,
                counters: Counters::default(),
                attachments: Vec::new(),
                v4: ObjectStateV4::from_card_def(card_def),
                spell_copy_origin: None,
                plotted_turn: None,
                zone_change_count: 0,
            });
            state.players[0].battlefield.push(id);
        }
        state
    }

    fn attacker_session(
        count: usize,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> RlEpisodeSessionV1 {
        let mut session =
            RlEpisodeSessionV1::reset_with_limits(23, 91, max_physical_decisions, max_policy_steps);
        session.state = attacker_state(count);
        session.surface = PolicySurfaceV5::new();
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.advance_to_decision_or_terminal();
        session
    }

    fn action_at(response: &RlSessionResponseV1, action_index: usize) -> (u64, u32, String) {
        let RlSessionResponseV1::Decision(decision) = response else {
            panic!("expected decision");
        };
        let action = &decision.legal_actions[action_index];
        (
            decision.step,
            action.selected_index,
            action.stable_id.clone(),
        )
    }

    fn first_action(response: &RlSessionResponseV1) -> (u64, u32, String) {
        action_at(response, 0)
    }

    #[test]
    fn policy_cap_preflights_the_whole_combat_group_and_exact_fit_admits_it() {
        let below_group = attacker_session(3, 8, 2);
        assert_eq!(below_group.policy_step_count(), 0);
        assert_eq!(below_group.physical_decision_count(), 0);
        assert!(below_group.current.is_none());
        assert!(!below_group.surface.scan_active());
        let RlSessionResponseV1::Terminal(terminal) = below_group.current_response() else {
            panic!("policy cap below the group must truncate before exposing it");
        };
        assert_eq!(terminal.policy_step_count, 0);
        assert_eq!(terminal.physical_decision_count, 0);
        assert_eq!(terminal.terminal_code, TerminalSafeCodeV2::DecisionCap);
        assert_eq!(terminal.terminal_reason, "policy_step_cap_reached:2");

        let mut exact_fit = attacker_session(3, 8, 3);
        assert!(exact_fit.surface.scan_active());
        for expected_substep in 0..3 {
            let response = exact_fit.current_response();
            let RlSessionResponseV1::Decision(decision) = &response else {
                panic!("exact-fit cap must admit every combat substep");
            };
            assert_eq!(decision.step, u64::from(expected_substep));
            assert_eq!(decision.physical_decision_id, 0);
            assert_eq!(decision.substep_index, expected_substep);
            assert_eq!(decision.substep_count, 3);
            assert_eq!(decision.legal_actions.len(), 2);
            let (step, index, id) = first_action(&response);
            exact_fit.step(23, step, index, &id).unwrap();
        }
        assert_eq!(exact_fit.policy_step_count(), 3);
        assert_eq!(exact_fit.physical_decision_count(), 1);
        assert!(!exact_fit.surface.scan_active());
        let RlSessionResponseV1::Terminal(terminal) = exact_fit.current_response() else {
            panic!("the empty-library combat fixture must terminate after the admitted group");
        };
        assert_eq!(terminal.policy_step_count, 3);
        assert_eq!(terminal.physical_decision_count, 1);
        assert_eq!(terminal.terminal_code, TerminalSafeCodeV2::NaturalGameOver);
        assert_eq!(terminal.terminal_reason, "game_over");
    }

    #[test]
    fn mid_combat_snapshot_restores_binding_response_and_next_group_transition() {
        let mut session = attacker_session(3, 8, 8);
        let start = session.current_response();
        let RlSessionResponseV1::Decision(start_decision) = &start else {
            panic!("expected first attacker inclusion");
        };
        assert_eq!(start_decision.physical_decision_id, 0);
        assert_eq!(start_decision.substep_index, 0);
        assert_eq!(start_decision.substep_count, 3);
        let (step, index, id) = action_at(&start, 1);
        session.step(23, step, index, &id).unwrap();

        let snapshot = session.snapshot_v5();
        let response_before = serde_json::to_vec(&session.current_response()).unwrap();
        let environment_before = session.privileged_environment_hash();
        assert_eq!(session.policy_step_count(), 1);
        assert_eq!(session.physical_decision_count(), 0);
        let RlSessionResponseV1::Decision(mid_decision) = session.current_response() else {
            panic!("snapshot must be mid-combat");
        };
        assert_eq!(mid_decision.step, 1);
        assert_eq!(mid_decision.physical_decision_id, 0);
        assert_eq!(mid_decision.substep_index, 1);
        assert_eq!(mid_decision.substep_count, 3);
        let (step, index, id) = first_action(&RlSessionResponseV1::Decision(mid_decision));

        let advanced = session.step(23, step, index, &id).unwrap();
        let advanced_bytes = serde_json::to_vec(&advanced).unwrap();
        let advanced_environment = session.privileged_environment_hash();
        let RlSessionResponseV1::Decision(advanced_decision) = &advanced else {
            panic!("second answer must advance within the same combat group");
        };
        assert_eq!(advanced_decision.step, 2);
        assert_eq!(advanced_decision.physical_decision_id, 0);
        assert_eq!(advanced_decision.substep_index, 2);
        assert_eq!(advanced_decision.substep_count, 3);
        assert_ne!(advanced_environment, environment_before);

        session.restore_v5(&snapshot);
        assert_eq!(
            serde_json::to_vec(&session.current_response()).unwrap(),
            response_before
        );
        assert_eq!(session.privileged_environment_hash(), environment_before);
        assert_eq!(session.policy_step_count(), 1);
        assert_eq!(session.physical_decision_count(), 0);
        assert!(session.surface.scan_active());

        let replayed = session.step(23, step, index, &id).unwrap();
        assert_eq!(serde_json::to_vec(&replayed).unwrap(), advanced_bytes);
        assert_eq!(session.privileged_environment_hash(), advanced_environment);
    }

    #[test]
    fn session_snapshot_restore_reproduces_response_hash_and_next_transition() {
        let mut session = RlEpisodeSessionV1::reset(17, 991, 64);
        let snapshot = session.snapshot_v5();
        let response_before = serde_json::to_vec(&session.current_response()).unwrap();
        let environment_before = session.privileged_environment_hash();
        let (step, index, id) = first_action(&session.current_response());

        let first_result =
            serde_json::to_vec(&session.step(17, step, index, &id).unwrap()).unwrap();
        assert_ne!(session.privileged_environment_hash(), environment_before);

        session.restore_v5(&snapshot);
        assert_eq!(
            serde_json::to_vec(&session.current_response()).unwrap(),
            response_before
        );
        assert_eq!(session.privileged_environment_hash(), environment_before);
        assert_eq!(
            serde_json::to_vec(&session.step(17, step, index, &id).unwrap()).unwrap(),
            first_result
        );
    }

    #[test]
    fn privileged_binding_rejects_state_surface_and_counter_drift_then_restores() {
        let mut session = RlEpisodeSessionV1::reset(5, 1234, 64);
        let snapshot = session.snapshot_v5();
        let (step, index, id) = first_action(&session.current_response());

        session.state.players[0].life -= 1;
        let err = session.step(5, step, index, &id).unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(
            err.message,
            "active decision no longer matches its privileged environment binding"
        );
        assert!(!err.message.contains("0x"));

        session.restore_v5(&snapshot);
        session.policy_step_count += 1;
        let err = session.step(5, step + 1, index, &id).unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);

        session.restore_v5(&snapshot);
        session.surface.reset_harness_context_for_test();
        let err = session.step(5, step, index, &id).unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);

        session.restore_v5(&snapshot);
        assert!(session.step(5, step, index, &id).is_ok());
    }
}
