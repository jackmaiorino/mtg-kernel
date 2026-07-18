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
use crate::phase_profile::{measure_optional, RlPhaseProfileV1, RlPhaseV1};
use crate::policy_surface_v5::{
    FastActorInPlaceApplyErrorV1, PolicyActionV5, PolicyDecisionV5, PolicySurfaceContextIdsV5,
    PolicySurfaceV5, POLICY_ENVIRONMENT_HASH_ALGORITHM, POLICY_SURFACE_VERSION,
};
use crate::rl::{
    build_deck_pair_state, core_policy_action_candidates_v5, legal_action_candidates_v5,
    observe_policy_v5, parse_strict_json_value, ActionSemanticV1, CorePolicyActionCandidateV1,
    EpisodeTerminalSummaryV1, LegalActionV5, ObservationV5, PlayerSeatV1,
    PolicyLegalActionCandidateV5, RlContractError, TerminalClassificationV1, TerminalOutcomeV1,
    TerminalSafeCodeV2,
};
use crate::runtime_decks::{runtime_deck_by_id, RuntimeDeckDefinition};
use crate::surface_v2::{SuppressionAuditMode, SurfaceDecision, H2_PREDICATE_VERSION};
use crate::KERNEL_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub const RL_SESSION_SCHEMA_VERSION: u32 = 5;
pub const RL_SESSION_PROTOCOL_VERSION: u32 = 5;
pub const RL_SESSION_PROTOCOL_NAME: &str = "kernel_rl_jsonl";
pub const FAST_ACTOR_CORE_ENVIRONMENT_HASH_ALGORITHM: &str =
    "fnv1a64-serde-json-fast-actor-core-environment-v1";
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

/// Allocation-light, in-process decision metadata for actor loops.
///
/// This type deliberately has no serde implementation and is not part of the
/// JSONL protocol. The only action handle exposed to an actor is its dense,
/// ordered index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastActorDecisionKindV1 {
    Surface,
    AttackerInclusion,
    BlockerInclusion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastActorDecisionV1 {
    pub episode_id: u64,
    pub step: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub acting_player: PlayerSeatV1,
    pub decision_kind: FastActorDecisionKindV1,
    pub legal_action_count: u32,
}

/// Non-wire response for [`FastActorSessionV1`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FastActorResponseV1 {
    Decision(FastActorDecisionV1),
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
    environment_revision: u64,
    bound_policy_step_count: u64,
    bound_physical_decision_count: u64,
}

#[derive(Debug, Clone)]
struct FastActorCurrentDecisionV1 {
    actor: PlayerId,
    decision_kind: FastActorDecisionKindV1,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    candidates: Vec<CorePolicyActionCandidateV1>,
    environment_revision: u64,
    bound_policy_step_count: u64,
    bound_physical_decision_count: u64,
}

/// One-use proof that an action came from the private fast actor's exact
/// current candidate vector. Its constructor is private to this module; the
/// policy surface can consume it but no sibling module can forge one from an
/// arbitrary `PolicyActionV5`.
pub(crate) struct FastActorCurrentCandidateProofV1 {
    owner_surface: *const PolicySurfaceV5,
    action: PolicyActionV5,
    current_revision: u64,
    next_revision: u64,
}

impl FastActorCurrentCandidateProofV1 {
    fn from_current(
        surface: &PolicySurfaceV5,
        current: &FastActorCurrentDecisionV1,
        selected_index: usize,
        next_revision: u64,
    ) -> Option<Self> {
        let selected = current.candidates.get(selected_index)?;
        Some(Self {
            owner_surface: std::ptr::from_ref(surface),
            action: selected.policy_action.clone(),
            current_revision: current.environment_revision,
            next_revision,
        })
    }

    pub(crate) fn into_parts(self) -> (*const PolicySurfaceV5, PolicyActionV5, u64, u64) {
        (
            self.owner_surface,
            self.action,
            self.current_revision,
            self.next_revision,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FastActorApplyPathV1 {
    InPlace,
    #[cfg(test)]
    CloneReference,
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
    environment_revision: u64,
    policy_step_count: u64,
    physical_decision_count: u64,
    current: Option<CurrentDecisionV1>,
    terminal: Option<RlSessionTerminalV1>,
}

#[derive(Clone)]
pub struct RlEpisodeSessionSnapshotV5(RlEpisodeSessionV1);

/// In-process actor lane that preserves the v5 policy surface and transition
/// semantics while omitting observations, visible hashes, stable/display
/// strings, and all JSON/Python work.
#[derive(Clone)]
pub struct FastActorSessionV1 {
    deck_ids: SessionDeckIdsV1,
    deck_hashes: SessionDeckHashesV1,
    episode_id: u64,
    max_physical_decisions: u64,
    max_policy_steps: u64,
    state: crate::state::GameState,
    surface: PolicySurfaceV5,
    environment_revision: u64,
    policy_step_count: u64,
    physical_decision_count: u64,
    current: Option<FastActorCurrentDecisionV1>,
    terminal: Option<RlSessionTerminalV1>,
}

#[derive(Clone)]
pub struct FastActorSessionSnapshotV1(FastActorSessionV1);

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
        Self::reset_with_decks_and_limits_profiled(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            None,
        )
    }

    fn reset_with_decks_and_limits_profiled(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        profile: Option<&mut RlPhaseProfileV1>,
    ) -> Result<Self, RlSessionError> {
        Self::reset_with_decks_and_limits_profiled_in_audit_mode(
            episode_id,
            env_seed,
            max_physical_decisions,
            max_policy_steps,
            deck_ids,
            profile,
            SuppressionAuditMode::Off,
        )
    }

    fn reset_with_decks_and_limits_profiled_in_audit_mode(
        episode_id: u64,
        env_seed: u64,
        max_physical_decisions: u64,
        max_policy_steps: u64,
        deck_ids: SessionDeckIdsV1,
        mut profile: Option<&mut RlPhaseProfileV1>,
        suppression_audit_mode: SuppressionAuditMode,
    ) -> Result<Self, RlSessionError> {
        let mut session = measure_optional(&mut profile, RlPhaseV1::Reset, || {
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
            Ok(RlEpisodeSessionV1 {
                deck_ids,
                deck_hashes,
                episode_id,
                max_physical_decisions,
                max_policy_steps,
                state,
                surface: if suppression_audit_mode == SuppressionAuditMode::Off {
                    PolicySurfaceV5::new_for_session()
                } else {
                    PolicySurfaceV5::new_with_suppression_audit_mode(suppression_audit_mode)
                },
                environment_revision: 0,
                policy_step_count: 0,
                physical_decision_count: 0,
                current: None,
                terminal: None,
            })
        })?;
        session.advance_to_decision_or_terminal_profiled(profile);
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

    /// Exact audit hash over the shared actor-relevant state, surface, binding,
    /// metadata, and ordered action semantics. Unlike the JSONL environment
    /// hash, this intentionally excludes ObservationV5 and stable action ids.
    pub fn privileged_core_environment_hash(&self) -> u64 {
        let current = self
            .current
            .as_ref()
            .map(|decision| CoreEnvironmentDecisionRefV1 {
                actor: decision.actor,
                physical_decision_id: decision.physical_decision_id,
                substep_index: decision.substep_index,
                substep_count: decision.substep_count,
                environment_revision: decision.environment_revision,
                bound_policy_step_count: decision.bound_policy_step_count,
                bound_physical_decision_count: decision.bound_physical_decision_count,
                legal_action_semantics: decision
                    .candidates
                    .iter()
                    .map(|candidate| &candidate.record.semantic)
                    .collect(),
            });
        compute_core_environment_hash(
            &self.state,
            &self.surface,
            self.environment_revision,
            self.policy_step_count,
            self.physical_decision_count,
            current,
        )
        .expect("session core environment serializes")
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
        self.apply_step_profiled(
            episode_id,
            expected_step,
            selected_index,
            selected_action_id,
            None,
        )?;
        Ok(self.current_response())
    }

    fn apply_step_profiled(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        selected_action_id: &str,
        mut profile: Option<&mut RlPhaseProfileV1>,
    ) -> Result<(), RlSessionError> {
        measure_optional(&mut profile, RlPhaseV1::StepValidation, || {
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
            Ok(())
        })?;
        let current = self
            .current
            .as_ref()
            .expect("nonterminal session has current decision");
        let next_environment_revision =
            measure_optional(&mut profile, RlPhaseV1::StepIntegrity, || {
                if current.environment_revision != self.environment_revision
                    || current.bound_policy_step_count != self.policy_step_count
                    || current.bound_physical_decision_count != self.physical_decision_count
                {
                    return Err(session_error(
                        RlSessionErrorCode::StaleEnvironmentBinding,
                        "active decision no longer matches its owned environment binding",
                    ));
                }
                self.environment_revision.checked_add(1).ok_or_else(|| {
                    session_error(
                        RlSessionErrorCode::StaleEnvironmentBinding,
                        "owned environment revision exhausted",
                    )
                })
            })?;
        let (policy_action, completes_physical) = measure_optional(
            &mut profile,
            RlPhaseV1::StepSelection,
            || {
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
                Ok((
                    selected.policy_action.clone(),
                    current.substep_index + 1 == current.substep_count,
                ))
            },
        )?;
        measure_optional(&mut profile, RlPhaseV1::StepApply, || {
            self.surface.apply_owned(
                &mut self.state,
                policy_action,
                self.environment_revision,
                next_environment_revision,
            )
        })
        .map_err(|_| {
            session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "selected action no longer matches the active policy environment",
            )
        })?;
        self.current = None;
        self.environment_revision = next_environment_revision;
        self.policy_step_count += 1;
        if completes_physical {
            self.physical_decision_count += 1;
        }
        self.advance_to_decision_or_terminal_profiled(profile);
        Ok(())
    }

    #[cfg(test)]
    fn advance_to_decision_or_terminal(&mut self) {
        self.advance_to_decision_or_terminal_profiled(None);
    }

    fn advance_to_decision_or_terminal_profiled(
        &mut self,
        mut profile: Option<&mut RlPhaseProfileV1>,
    ) {
        self.current = None;
        let surfaced = match measure_optional(&mut profile, RlPhaseV1::Advance, || {
            self.surface
                .next_decision_owned(&mut self.state, self.environment_revision)
        }) {
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
        let observation = match measure_optional(&mut profile, RlPhaseV1::Observe, || {
            observe_policy_v5(
                &self.state,
                &self.surface,
                actor,
                self.policy_step_count,
                self.physical_decision_count,
                substep_index,
                substep_count,
            )
        }) {
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
        let candidates = match measure_optional(&mut profile, RlPhaseV1::Actions, || {
            legal_action_candidates_v5(&surfaced, &self.state)
        }) {
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
        measure_optional(&mut profile, RlPhaseV1::Postbind, || {
            self.current = Some(CurrentDecisionV1 {
                actor,
                physical_decision_id: self.physical_decision_count,
                substep_index,
                substep_count,
                observation,
                candidates,
                environment_revision: self.environment_revision,
                bound_policy_step_count: self.policy_step_count,
                bound_physical_decision_count: self.physical_decision_count,
            });
        });
    }

    fn compute_environment_hash(&self, current: Option<&CurrentDecisionV1>) -> Result<u64, String> {
        #[cfg(test)]
        TEST_EXACT_ENVIRONMENT_HASH_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));

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

impl FastActorSessionV1 {
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
        let mut session = FastActorSessionV1 {
            deck_ids,
            deck_hashes,
            episode_id,
            max_physical_decisions,
            max_policy_steps,
            state,
            surface: PolicySurfaceV5::new_for_session(),
            environment_revision: 0,
            policy_step_count: 0,
            physical_decision_count: 0,
            current: None,
            terminal: None,
        };
        session.advance_to_decision_or_terminal();
        Ok(session)
    }

    pub fn current_response(&self) -> FastActorResponseV1 {
        if let Some(terminal) = &self.terminal {
            return FastActorResponseV1::Terminal(terminal.clone());
        }
        let current = self
            .current
            .as_ref()
            .expect("session has either a current decision or terminal");
        FastActorResponseV1::Decision(FastActorDecisionV1 {
            episode_id: self.episode_id,
            step: self.policy_step_count,
            physical_decision_id: current.physical_decision_id,
            substep_index: current.substep_index,
            substep_count: current.substep_count,
            acting_player: current.actor.into(),
            decision_kind: current.decision_kind,
            legal_action_count: u32::try_from(current.candidates.len())
                .expect("fast actor candidate count was checked when bound"),
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

    /// Audit-only counterpart to
    /// [`RlEpisodeSessionV1::privileged_core_environment_hash`]. It is never
    /// computed by reset/step and therefore does not tax the actor loop.
    pub fn privileged_core_environment_hash(&self) -> u64 {
        let current = self
            .current
            .as_ref()
            .map(|decision| CoreEnvironmentDecisionRefV1 {
                actor: decision.actor,
                physical_decision_id: decision.physical_decision_id,
                substep_index: decision.substep_index,
                substep_count: decision.substep_count,
                environment_revision: decision.environment_revision,
                bound_policy_step_count: decision.bound_policy_step_count,
                bound_physical_decision_count: decision.bound_physical_decision_count,
                legal_action_semantics: decision
                    .candidates
                    .iter()
                    .map(|candidate| &candidate.semantic)
                    .collect(),
            });
        compute_core_environment_hash(
            &self.state,
            &self.surface,
            self.environment_revision,
            self.policy_step_count,
            self.physical_decision_count,
            current,
        )
        .expect("fast actor core environment serializes")
    }

    pub fn snapshot_v1(&self) -> FastActorSessionSnapshotV1 {
        FastActorSessionSnapshotV1(self.clone())
    }

    pub fn restore_v1(&mut self, snapshot: &FastActorSessionSnapshotV1) {
        *self = snapshot.0.clone();
    }

    pub fn step(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlSessionError> {
        self.step_with_apply_path(
            episode_id,
            expected_step,
            selected_index,
            FastActorApplyPathV1::InPlace,
        )
    }

    #[cfg(test)]
    fn step_clone_reference(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
    ) -> Result<FastActorResponseV1, RlSessionError> {
        self.step_with_apply_path(
            episode_id,
            expected_step,
            selected_index,
            FastActorApplyPathV1::CloneReference,
        )
    }

    fn step_with_apply_path(
        &mut self,
        episode_id: u64,
        expected_step: u64,
        selected_index: u32,
        apply_path: FastActorApplyPathV1,
    ) -> Result<FastActorResponseV1, RlSessionError> {
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
        if current.environment_revision != self.environment_revision
            || current.bound_policy_step_count != self.policy_step_count
            || current.bound_physical_decision_count != self.physical_decision_count
        {
            return Err(session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "active decision no longer matches its owned environment binding",
            ));
        }
        let next_environment_revision =
            self.environment_revision.checked_add(1).ok_or_else(|| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "owned environment revision exhausted",
                )
            })?;
        let next_policy_step_count = self.policy_step_count.checked_add(1).ok_or_else(|| {
            session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "owned policy step counter exhausted",
            )
        })?;
        let completes_physical =
            current.substep_index.checked_add(1) == Some(current.substep_count);
        let next_physical_decision_count = if completes_physical {
            self.physical_decision_count.checked_add(1).ok_or_else(|| {
                session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "owned physical decision counter exhausted",
                )
            })?
        } else {
            self.physical_decision_count
        };
        let selected_index = selected_index as usize;
        if current.candidates.get(selected_index).is_none() {
            return Err(session_error(
                RlSessionErrorCode::SelectedIndexOutOfRange,
                "selected_index is outside the current legal action list",
            ));
        }

        let apply_result = match apply_path {
            FastActorApplyPathV1::InPlace => {
                let proof = FastActorCurrentCandidateProofV1::from_current(
                    &self.surface,
                    current,
                    selected_index,
                    next_environment_revision,
                )
                .expect("selected candidate was checked above");
                self.surface
                    .apply_fast_actor_current_candidate_in_place(&mut self.state, proof)
            }
            #[cfg(test)]
            FastActorApplyPathV1::CloneReference => self
                .surface
                .apply_owned(
                    &mut self.state,
                    current.candidates[selected_index].policy_action.clone(),
                    self.environment_revision,
                    next_environment_revision,
                )
                .map_err(|_| FastActorInPlaceApplyErrorV1::RejectedBeforeMutation),
        };
        if let Err(error) = apply_result {
            if error == FastActorInPlaceApplyErrorV1::InternalApplyFailure
                && apply_path == FastActorApplyPathV1::InPlace
            {
                // A bound action can fail here only if an internal invariant
                // changed after prevalidation. Do not expose a retryable
                // decision from a potentially partially mutated environment.
                self.current = None;
                self.terminal = Some(halted_terminal(
                    &self.deck_ids,
                    self.deck_hashes,
                    self.episode_id,
                    "fail_closed:fast_actor_in_place_apply".to_string(),
                    self.policy_step_count,
                    self.physical_decision_count,
                ));
                return Err(session_error(
                    RlSessionErrorCode::StaleEnvironmentBinding,
                    "prevalidated fast actor action failed internally; episode halted",
                ));
            }
            return Err(session_error(
                RlSessionErrorCode::StaleEnvironmentBinding,
                "selected action no longer matches the active policy environment",
            ));
        }
        self.current = None;
        self.environment_revision = next_environment_revision;
        self.policy_step_count = next_policy_step_count;
        self.physical_decision_count = next_physical_decision_count;
        self.advance_to_decision_or_terminal();
        Ok(self.current_response())
    }

    fn advance_to_decision_or_terminal(&mut self) {
        self.current = None;
        let surfaced = match self
            .surface
            .next_decision_owned(&mut self.state, self.environment_revision)
        {
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
        let candidates = match core_policy_action_candidates_v5(&surfaced, &self.state) {
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
        if u32::try_from(candidates.len()).is_err() {
            self.terminal = Some(halted_terminal(
                &self.deck_ids,
                self.deck_hashes,
                self.episode_id,
                "fail_closed:policy action index exceeds u32".to_string(),
                self.policy_step_count,
                self.physical_decision_count,
            ));
            return;
        }
        let decision_kind = match surfaced {
            PolicyDecisionV5::Surface(_) => FastActorDecisionKindV1::Surface,
            PolicyDecisionV5::AttackerInclusion { .. } => {
                FastActorDecisionKindV1::AttackerInclusion
            }
            PolicyDecisionV5::BlockerInclusion { .. } => FastActorDecisionKindV1::BlockerInclusion,
        };
        self.current = Some(FastActorCurrentDecisionV1 {
            actor,
            decision_kind,
            physical_decision_id: self.physical_decision_count,
            substep_index,
            substep_count,
            candidates,
            environment_revision: self.environment_revision,
            bound_policy_step_count: self.policy_step_count,
            bound_physical_decision_count: self.physical_decision_count,
        });
    }
}

#[derive(Serialize)]
struct CoreEnvironmentDecisionRefV1<'a> {
    actor: PlayerId,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
    environment_revision: u64,
    bound_policy_step_count: u64,
    bound_physical_decision_count: u64,
    legal_action_semantics: Vec<&'a ActionSemanticV1>,
}

fn compute_core_environment_hash(
    state: &crate::state::GameState,
    surface: &PolicySurfaceV5,
    environment_revision: u64,
    policy_step_count: u64,
    physical_decision_count: u64,
    current: Option<CoreEnvironmentDecisionRefV1<'_>>,
) -> Result<u64, String> {
    #[derive(Serialize)]
    struct CoreEnvironmentEnvelopeV1<'a> {
        schema_version: u32,
        hash_algorithm: &'static str,
        diagnostic_state_hash_algorithm: &'static str,
        diagnostic_state_hash: u64,
        harness_surface_context: crate::surface_v2::HarnessSurfacePublicContextV2,
        policy_surface_context: PolicySurfaceContextIdsV5,
        environment_revision: u64,
        policy_step_count: u64,
        physical_decision_count: u64,
        current: Option<CoreEnvironmentDecisionRefV1<'a>>,
    }

    let envelope = CoreEnvironmentEnvelopeV1 {
        schema_version: 1,
        hash_algorithm: FAST_ACTOR_CORE_ENVIRONMENT_HASH_ALGORITHM,
        diagnostic_state_hash_algorithm: crate::state::DIAGNOSTIC_STATE_HASH_ALGORITHM,
        diagnostic_state_hash: state.diagnostic_state_hash(),
        harness_surface_context: surface.harness_public_context(),
        policy_surface_context: surface.privileged_scan_context()?,
        environment_revision,
        policy_step_count,
        physical_decision_count,
        current,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(|err| err.to_string())?;
    Ok(fnv1a64(&bytes))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
thread_local! {
    static TEST_EXACT_ENVIRONMENT_HASH_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn reset_test_exact_environment_hash_calls() {
    TEST_EXACT_ENVIRONMENT_HASH_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
fn test_exact_environment_hash_calls() -> u64 {
    TEST_EXACT_ENVIRONMENT_HASH_CALLS.with(std::cell::Cell::get)
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

enum RetryDispositionV1 {
    Cached,
    ReuseMismatch(Box<KernelRlResponseV1>),
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
        self.handle_line_impl(line, None)
    }

    pub fn handle_line_profiled(&mut self, line: &str, profile: &mut RlPhaseProfileV1) -> String {
        self.handle_line_impl(line, Some(profile))
    }

    fn handle_line_impl(
        &mut self,
        line: &str,
        mut profile: Option<&mut RlPhaseProfileV1>,
    ) -> String {
        if let Some(profile) = profile.as_deref_mut() {
            profile.request_lines = profile.request_lines.saturating_add(1);
        }
        let value = match measure_optional(&mut profile, RlPhaseV1::Parse, || {
            parse_strict_json_value(line)
        }) {
            Ok(value) => value,
            Err(_) => {
                if serde_json::from_str::<serde::de::IgnoredAny>(line).is_ok() {
                    return serialize_response_profiled(
                        error_response(
                            None,
                            "malformed_request",
                            "request does not match the v5 protocol schema",
                        ),
                        &mut profile,
                    );
                }
                return serialize_response_profiled(
                    error_response(None, "malformed_json", "request line is not valid JSON"),
                    &mut profile,
                );
            }
        };
        let request_id = request_id_from_value(&value);
        let request = match measure_optional(&mut profile, RlPhaseV1::Decode, || {
            serde_json::from_value::<KernelRlRequestV1>(value)
        }) {
            Ok(request) => request,
            Err(_) => {
                return serialize_response_profiled(
                    error_response(
                        request_id,
                        "malformed_request",
                        "request does not match the v5 protocol schema",
                    ),
                    &mut profile,
                );
            }
        };
        if let Some(profile) = profile.as_deref_mut() {
            match &request {
                KernelRlRequestV1::Reset { .. } => {
                    profile.reset_requests = profile.reset_requests.saturating_add(1)
                }
                KernelRlRequestV1::Step { .. } => {
                    profile.step_requests = profile.step_requests.saturating_add(1)
                }
            }
        }
        let retry = measure_optional(&mut profile, RlPhaseV1::Retry, || {
            self.last_exchange.as_ref().and_then(|cached| {
                if cached.request_id != request.request_id() {
                    return None;
                }
                if cached.request == request {
                    Some(RetryDispositionV1::Cached)
                } else {
                    Some(RetryDispositionV1::ReuseMismatch(Box::new(error_response(
                        Some(request.request_id().to_string()),
                        "request_id_reuse_mismatch",
                        "request_id was reused for a different immediate request payload",
                    ))))
                }
            })
        });
        if let Some(retry) = retry {
            return match retry {
                RetryDispositionV1::Cached => {
                    measure_optional(&mut profile, RlPhaseV1::Response, || ());
                    let response_line =
                        measure_optional(&mut profile, RlPhaseV1::Serialize, || {
                            self.last_exchange
                                .as_ref()
                                .expect("cached retry has an exchange")
                                .response_line
                                .clone()
                        });
                    if let Some(profile) = profile.as_deref_mut() {
                        profile.response_lines = profile.response_lines.saturating_add(1);
                    }
                    response_line
                }
                RetryDispositionV1::ReuseMismatch(response) => {
                    let response =
                        measure_optional(&mut profile, RlPhaseV1::Response, || *response);
                    serialize_response_profiled(response, &mut profile)
                }
            };
        }
        let should_cache = request.schema_version() == RL_SESSION_SCHEMA_VERSION;
        let request_for_cache = request.clone();
        let response = self.handle_request_profiled(request, profile.as_deref_mut());
        let response_line = serialize_response_profiled(response, &mut profile);
        if should_cache {
            self.last_exchange = Some(CachedProtocolExchangeV1 {
                request_id: request_for_cache.request_id().to_string(),
                request: request_for_cache,
                response_line: response_line.clone(),
            });
        }
        response_line
    }

    fn handle_request_profiled(
        &mut self,
        request: KernelRlRequestV1,
        mut profile: Option<&mut RlPhaseProfileV1>,
    ) -> KernelRlResponseV1 {
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
                    return measure_optional(&mut profile, RlPhaseV1::Response, || {
                        error_response(
                            Some(request_id),
                            "schema_version_mismatch",
                            "unsupported request schema_version",
                        )
                    });
                }
                let session = match RlEpisodeSessionV1::reset_with_decks_and_limits_profiled(
                    episode_id,
                    env_seed,
                    max_physical_decisions,
                    max_policy_steps,
                    deck_ids,
                    profile.as_deref_mut(),
                ) {
                    Ok(session) => session,
                    Err(err) => {
                        return measure_optional(&mut profile, RlPhaseV1::Response, || {
                            error_response(
                                Some(request_id),
                                session_error_code(&err.code),
                                &err.message,
                            )
                        });
                    }
                };
                let response = measure_optional(&mut profile, RlPhaseV1::Response, || {
                    session_response_to_protocol(request_id, session.current_response())
                });
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
                    return measure_optional(&mut profile, RlPhaseV1::Response, || {
                        error_response(
                            Some(request_id),
                            "schema_version_mismatch",
                            "unsupported request schema_version",
                        )
                    });
                }
                let Some(session) = self.session.as_mut() else {
                    return measure_optional(&mut profile, RlPhaseV1::Response, || {
                        error_response(
                            Some(request_id),
                            "step_before_reset",
                            "step request received before reset",
                        )
                    });
                };
                match session.apply_step_profiled(
                    episode_id,
                    expected_step,
                    selected_index,
                    &selected_action_id,
                    profile.as_deref_mut(),
                ) {
                    Ok(()) => measure_optional(&mut profile, RlPhaseV1::Response, || {
                        session_response_to_protocol(request_id, session.current_response())
                    }),
                    Err(err) => measure_optional(&mut profile, RlPhaseV1::Response, || {
                        error_response(
                            Some(request_id),
                            session_error_code(&err.code),
                            &err.message,
                        )
                    }),
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

fn serialize_response_profiled(
    response: KernelRlResponseV1,
    profile: &mut Option<&mut RlPhaseProfileV1>,
) -> String {
    let line = measure_optional(profile, RlPhaseV1::Serialize, || {
        serialize_response(response)
    });
    if let Some(profile) = profile.as_deref_mut() {
        profile.response_lines = profile.response_lines.saturating_add(1);
    }
    line
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
    use crate::policy_surface_v5::{
        reset_test_exact_surface_hash_calls, test_exact_surface_hash_calls,
    };
    use crate::rl::{
        card_name, make_legal_action_v5, reset_test_policy_v5_materialization_calls,
        test_policy_v5_materialization_calls, validate_core_policy_action_candidates_v5,
        ActionSemanticV1,
    };
    use crate::state::{Counters, GameObject, GameState, ObjectStateV4, SplitMix64, Step, Zone};

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

    fn blocker_state(count: usize) -> GameState {
        let mut state = attacker_state(1);
        let attacker = state.players[0].battlefield[0];
        state.step = Step::DeclareBlockers;
        state.priority_player = PlayerId::P1;
        state.engine.combat.attackers_declared = true;
        state.engine.combat.attackers = vec![attacker];
        state.engine.combat.blockers_declared = false;
        let card_def = card_id_by_name("Voldaren Epicure").unwrap();
        for _ in 0..count {
            let id = state.objects.push(GameObject {
                card_def,
                name: "Voldaren Epicure".to_string(),
                owner: PlayerId::P1,
                controller: PlayerId::P1,
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
            state.players[1].battlefield.push(id);
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
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.advance_to_decision_or_terminal();
        session
    }

    fn fast_attacker_session(
        count: usize,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> FastActorSessionV1 {
        let mut session =
            FastActorSessionV1::reset_with_limits(23, 91, max_physical_decisions, max_policy_steps);
        session.state = attacker_state(count);
        session.surface = PolicySurfaceV5::new();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.advance_to_decision_or_terminal();
        session
    }

    fn fast_blocker_session(
        count: usize,
        max_physical_decisions: u64,
        max_policy_steps: u64,
    ) -> FastActorSessionV1 {
        let mut session =
            FastActorSessionV1::reset_with_limits(23, 91, max_physical_decisions, max_policy_steps);
        session.state = blocker_state(count);
        session.surface = PolicySurfaceV5::new();
        session.environment_revision = 0;
        session.policy_step_count = 0;
        session.physical_decision_count = 0;
        session.current = None;
        session.terminal = None;
        session.advance_to_decision_or_terminal();
        session
    }

    fn fast_actor_audit_bytes(session: &FastActorSessionV1) -> Vec<u8> {
        serde_json::to_vec(&(
            (
                &session.deck_ids,
                session.deck_hashes,
                session.episode_id,
                session.max_physical_decisions,
                session.max_policy_steps,
            ),
            (
                &session.state,
                session.surface.harness_public_context(),
                session.surface.privileged_scan_context().unwrap(),
            ),
            (
                session.environment_revision,
                session.policy_step_count,
                session.physical_decision_count,
                session.privileged_core_environment_hash(),
                format!("{:?}", session.current_response()),
                format!(
                    "{:?}",
                    session
                        .current
                        .as_ref()
                        .map(|decision| &decision.candidates)
                ),
                &session.terminal,
            ),
        ))
        .unwrap()
    }

    fn assert_fast_actor_rejection_is_byte_atomic(
        session: &mut FastActorSessionV1,
        expected_code: RlSessionErrorCode,
        request: impl FnOnce(&mut FastActorSessionV1) -> Result<FastActorResponseV1, RlSessionError>,
    ) {
        let before = fast_actor_audit_bytes(session);
        let error = request(session).unwrap_err();
        assert_eq!(error.code, expected_code);
        assert_eq!(
            fast_actor_audit_bytes(session),
            before,
            "rejected fast-actor input mutated the audit snapshot"
        );
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
            let revision_before = exact_fit.environment_revision;
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
            assert_eq!(
                exact_fit.environment_revision,
                revision_before + 1,
                "every successful policy step advances exactly one owned revision"
            );
            if let Some(current) = &exact_fit.current {
                assert_eq!(current.environment_revision, exact_fit.environment_revision);
                assert_eq!(current.bound_policy_step_count, exact_fit.policy_step_count);
                assert_eq!(
                    current.bound_physical_decision_count,
                    exact_fit.physical_decision_count
                );
            }
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
        let revision_before = session.environment_revision;
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
        assert_eq!(session.environment_revision, revision_before + 1);
        let RlSessionResponseV1::Decision(advanced_decision) = &advanced else {
            panic!("second answer must advance within the same combat group");
        };
        assert_eq!(advanced_decision.step, 2);
        assert_eq!(advanced_decision.physical_decision_id, 0);
        assert_eq!(advanced_decision.substep_index, 2);
        assert_eq!(advanced_decision.substep_count, 3);
        assert_ne!(advanced_environment, environment_before);

        session.restore_v5(&snapshot);
        assert_eq!(session.environment_revision, revision_before);
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
    fn privileged_exact_audit_detects_private_state_surface_and_counter_drift_then_restores() {
        let mut session = RlEpisodeSessionV1::reset(5, 1234, 64);
        let snapshot = session.snapshot_v5();
        let exact = session.privileged_environment_hash();

        session.state.players[0].life -= 1;
        assert_ne!(session.privileged_environment_hash(), exact);

        session.restore_v5(&snapshot);
        session.policy_step_count += 1;
        assert_ne!(session.privileged_environment_hash(), exact);

        session.restore_v5(&snapshot);
        session.surface.reset_harness_context_for_test();
        assert_ne!(session.privileged_environment_hash(), exact);

        session.restore_v5(&snapshot);
        assert_eq!(session.privileged_environment_hash(), exact);
    }

    #[test]
    fn owned_revision_and_counter_poisoning_precede_invalid_selection_then_restore() {
        let mut session = RlEpisodeSessionV1::reset(5, 1234, 64);
        let snapshot = session.snapshot_v5();
        let (step, index, id) = first_action(&session.current_response());

        session.current.as_mut().unwrap().environment_revision += 1;
        let err = session
            .step(5, step, u32::MAX, "unknown-action")
            .unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(
            err.message,
            "active decision no longer matches its owned environment binding"
        );
        assert!(!err.message.contains("0x"));

        session.restore_v5(&snapshot);
        session.policy_step_count += 1;
        let err = session
            .step(5, step + 1, u32::MAX, "unknown-action")
            .unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);

        session.restore_v5(&snapshot);
        session.physical_decision_count += 1;
        let err = session
            .step(5, step, u32::MAX, "unknown-action")
            .unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);

        session.restore_v5(&snapshot);
        assert!(session.step(5, step, index, &id).is_ok());
    }

    #[test]
    fn owned_revision_overflow_fails_before_selection_and_is_nonmutating() {
        let mut session = RlEpisodeSessionV1::reset(5, 1234, 64);
        session.environment_revision = u64::MAX;
        session.current.as_mut().unwrap().environment_revision = u64::MAX;
        let response_before = serde_json::to_vec(&session.current_response()).unwrap();
        let state_before = session.state.clone();
        let harness_before = session.surface.harness_public_context();
        let scan_before = session.surface.privileged_scan_context().unwrap();

        let err = session
            .step(5, session.policy_step_count, u32::MAX, "unknown-action")
            .unwrap_err();
        assert_eq!(err.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(err.message, "owned environment revision exhausted");
        assert_eq!(session.environment_revision, u64::MAX);
        assert_eq!(session.policy_step_count, 0);
        assert_eq!(session.physical_decision_count, 0);
        assert_eq!(session.state, state_before);
        assert_eq!(session.surface.harness_public_context(), harness_before);
        assert_eq!(
            session.surface.privileged_scan_context().unwrap(),
            scan_before
        );
        assert_eq!(
            serde_json::to_vec(&session.current_response()).unwrap(),
            response_before
        );
    }

    #[test]
    fn owned_reset_and_combat_steps_compute_no_exact_hashes_but_audit_remains_available() {
        reset_test_exact_environment_hash_calls();
        reset_test_exact_surface_hash_calls();

        let mut session = attacker_session(3, 8, 8);
        assert_eq!(test_exact_environment_hash_calls(), 0);
        assert_eq!(test_exact_surface_hash_calls(), 0);

        for _ in 0..3 {
            let response = session.current_response();
            let (step, index, id) = first_action(&response);
            session.step(23, step, index, &id).unwrap();
        }
        assert_eq!(test_exact_environment_hash_calls(), 0);
        assert_eq!(
            test_exact_surface_hash_calls(),
            0,
            "owned combat scans must never enter the standalone exact-hash path"
        );

        let _ = session.privileged_environment_hash();
        assert_eq!(test_exact_environment_hash_calls(), 1);
        assert_eq!(test_exact_surface_hash_calls(), 0);
    }

    #[test]
    fn failed_final_owned_combat_commit_does_not_advance_revision_or_counters() {
        let mut session = attacker_session(1, 8, 8);
        let snapshot = session.snapshot_v5();
        let response = session.current_response();
        let (step, include_index, include_id) = action_at(&response, 1);

        session.state.players[0].battlefield.clear();
        let tampered_state = session.state.clone();
        let response_before = serde_json::to_vec(&session.current_response()).unwrap();
        let revision_before = session.environment_revision;
        let error = session
            .step(23, step, include_index, &include_id)
            .unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(session.environment_revision, revision_before);
        assert_eq!(session.policy_step_count, 0);
        assert_eq!(session.physical_decision_count, 0);
        assert_eq!(session.state, tampered_state);
        assert_eq!(
            serde_json::to_vec(&session.current_response()).unwrap(),
            response_before
        );

        session.restore_v5(&snapshot);
        assert!(session.step(23, step, include_index, &include_id).is_ok());
    }

    #[test]
    fn fast_actor_cap_snapshot_retry_overflow_and_final_commit_are_fail_closed() {
        let full_below = attacker_session(3, 8, 2);
        let fast_below = fast_attacker_session(3, 8, 2);
        let RlSessionResponseV1::Terminal(full_terminal) = full_below.current_response() else {
            panic!("full cap fixture must terminate");
        };
        let FastActorResponseV1::Terminal(fast_terminal) = fast_below.current_response() else {
            panic!("fast cap fixture must terminate");
        };
        assert_eq!(fast_terminal, full_terminal);
        assert!(!fast_below.surface.scan_active());

        let mut session = fast_attacker_session(3, 8, 3);
        let response_before = session.current_response();
        let state_before = session.state.clone();
        let surface_before = session.surface.clone();
        let binding_before = session.privileged_core_environment_hash();
        let error = session.step(23, 0, u32::MAX).unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::SelectedIndexOutOfRange);
        assert_eq!(session.current_response(), response_before);
        assert_eq!(session.state, state_before);
        assert_eq!(
            session.surface.harness_public_context(),
            surface_before.harness_public_context()
        );
        assert_eq!(session.privileged_core_environment_hash(), binding_before);

        let snapshot = session.snapshot_v1();
        session.current.as_mut().unwrap().environment_revision += 1;
        let error = session.step(23, 0, u32::MAX).unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        session.restore_v1(&snapshot);
        let advanced = session.step(23, 0, 1).unwrap();
        let FastActorResponseV1::Decision(advanced) = advanced else {
            panic!("first combat answer must expose the second substep");
        };
        assert_eq!(advanced.step, 1);
        assert_eq!(advanced.physical_decision_id, 0);
        assert_eq!(advanced.substep_index, 1);
        let advanced_hash = session.privileged_core_environment_hash();
        session.restore_v1(&snapshot);
        assert_eq!(
            session.step(23, 0, 1).unwrap(),
            FastActorResponseV1::Decision(advanced)
        );
        assert_eq!(session.privileged_core_environment_hash(), advanced_hash);

        session.restore_v1(&snapshot);
        session.environment_revision = u64::MAX;
        session.current.as_mut().unwrap().environment_revision = u64::MAX;
        let overflow_before = session.privileged_core_environment_hash();
        let error = session.step(23, 0, u32::MAX).unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(error.message, "owned environment revision exhausted");
        assert_eq!(session.privileged_core_environment_hash(), overflow_before);

        let mut final_commit = fast_attacker_session(1, 8, 8);
        let final_snapshot = final_commit.snapshot_v1();
        final_commit.state.players[0].battlefield.clear();
        let tampered_state = final_commit.state.clone();
        let response_before = final_commit.current_response();
        let audit_before = fast_actor_audit_bytes(&final_commit);
        let revision_before = final_commit.environment_revision;
        let error = final_commit.step(23, 0, 1).unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(final_commit.environment_revision, revision_before);
        assert_eq!(final_commit.policy_step_count, 0);
        assert_eq!(final_commit.physical_decision_count, 0);
        assert_eq!(final_commit.state, tampered_state);
        assert_eq!(final_commit.current_response(), response_before);
        assert_eq!(fast_actor_audit_bytes(&final_commit), audit_before);
        final_commit.restore_v1(&final_snapshot);
        assert!(final_commit.step(23, 0, 1).is_ok());
    }

    #[test]
    fn fast_actor_final_blocker_tamper_is_nonpanicking_and_byte_atomic() {
        let mut session = fast_blocker_session(1, 8, 8);
        let snapshot = session.snapshot_v1();
        let blocker = session.state.players[1].battlefield[0];
        session.state.players[1].battlefield.clear();
        session.state.objects.get_mut(blocker).zone = Zone::Graveyard;
        let response_before = session.current_response();
        let audit_before = fast_actor_audit_bytes(&session);

        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| session.step(23, 0, 1)));
        assert!(
            caught.is_ok(),
            "stale final blocker aggregate must not panic"
        );
        let error = caught.unwrap().unwrap_err();
        assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        assert_eq!(session.current_response(), response_before);
        assert_eq!(fast_actor_audit_bytes(&session), audit_before);

        session.restore_v1(&snapshot);
        assert!(session.step(23, 0, 1).is_ok());
    }

    #[test]
    fn fast_actor_rejected_inputs_are_byte_atomic_before_in_place_apply() {
        let mut session = fast_attacker_session(3, 8, 8);
        let mut candidate_payload_tamper = session.clone();
        let payload_before = fast_actor_audit_bytes(&candidate_payload_tamper);
        candidate_payload_tamper
            .current
            .as_mut()
            .unwrap()
            .candidates[0]
            .policy_action = PolicyActionV5::Surface(crate::surface_v2::SurfaceAction::Action(
            crate::engine::Action::Pass,
        ));
        assert_ne!(
            fast_actor_audit_bytes(&candidate_payload_tamper),
            payload_before,
            "audit bytes must bind the full executable candidate payload"
        );
        assert_fast_actor_rejection_is_byte_atomic(
            &mut candidate_payload_tamper,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |candidate_payload_tamper| candidate_payload_tamper.step(23, 0, 0),
        );

        assert_fast_actor_rejection_is_byte_atomic(
            &mut session,
            RlSessionErrorCode::EpisodeIdMismatch,
            |session| session.step(24, 0, 0),
        );
        assert_fast_actor_rejection_is_byte_atomic(
            &mut session,
            RlSessionErrorCode::ExpectedStepMismatch,
            |session| session.step(23, 1, 0),
        );
        assert_fast_actor_rejection_is_byte_atomic(
            &mut session,
            RlSessionErrorCode::SelectedIndexOutOfRange,
            |session| session.step(23, 0, u32::MAX),
        );

        let mut stale = session.clone();
        stale.current.as_mut().unwrap().environment_revision += 1;
        assert_fast_actor_rejection_is_byte_atomic(
            &mut stale,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |stale| stale.step(23, 0, u32::MAX),
        );

        let mut revision_overflow = session.clone();
        revision_overflow.environment_revision = u64::MAX;
        revision_overflow
            .current
            .as_mut()
            .unwrap()
            .environment_revision = u64::MAX;
        assert_fast_actor_rejection_is_byte_atomic(
            &mut revision_overflow,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |revision_overflow| revision_overflow.step(23, 0, u32::MAX),
        );

        let mut policy_overflow = session.clone();
        policy_overflow.policy_step_count = u64::MAX;
        policy_overflow
            .current
            .as_mut()
            .unwrap()
            .bound_policy_step_count = u64::MAX;
        assert_fast_actor_rejection_is_byte_atomic(
            &mut policy_overflow,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |policy_overflow| policy_overflow.step(23, u64::MAX, u32::MAX),
        );

        let mut physical_overflow = fast_attacker_session(1, u64::MAX, 8);
        physical_overflow.physical_decision_count = u64::MAX;
        physical_overflow
            .current
            .as_mut()
            .unwrap()
            .bound_physical_decision_count = u64::MAX;
        assert_fast_actor_rejection_is_byte_atomic(
            &mut physical_overflow,
            RlSessionErrorCode::StaleEnvironmentBinding,
            |physical_overflow| physical_overflow.step(23, 0, u32::MAX),
        );

        session.step(23, 0, 1).unwrap();
        assert_fast_actor_rejection_is_byte_atomic(
            &mut session,
            RlSessionErrorCode::ExpectedStepMismatch,
            |session| session.step(23, 0, 1),
        );

        let mut capped = fast_attacker_session(3, 0, 8);
        assert_fast_actor_rejection_is_byte_atomic(
            &mut capped,
            RlSessionErrorCode::EpisodeAlreadyTerminal,
            |capped| capped.step(23, 0, 0),
        );
    }

    #[test]
    fn fast_actor_candidate_proof_cannot_cross_same_revision_surfaces() {
        let source = fast_attacker_session(3, 8, 8);
        let current = source.current.as_ref().unwrap();
        let proof = FastActorCurrentCandidateProofV1::from_current(
            &source.surface,
            current,
            1,
            source.environment_revision + 1,
        )
        .unwrap();
        let mut other = source.clone();
        let before = fast_actor_audit_bytes(&other);

        let error = other
            .surface
            .apply_fast_actor_current_candidate_in_place(&mut other.state, proof)
            .unwrap_err();
        assert_eq!(error, FastActorInPlaceApplyErrorV1::RejectedBeforeMutation);
        assert_eq!(fast_actor_audit_bytes(&other), before);
    }

    #[test]
    fn fast_actor_reset_and_steps_materialize_zero_observations_or_stable_ids() {
        reset_test_policy_v5_materialization_calls();
        let mut fast = fast_attacker_session(3, 8, 8);
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));
        for step in 0..3 {
            fast.step(23, step, 1).unwrap();
        }
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));

        let _full = attacker_session(3, 8, 8);
        let (observations, stable_actions) = test_policy_v5_materialization_calls();
        assert!(observations > 0);
        assert!(stable_actions > 0);
    }

    #[test]
    fn shared_core_and_full_v5_fail_identically_on_forbidden_or_duplicate_semantics() {
        reset_test_policy_v5_materialization_calls();
        let state = attacker_state(1);
        let object = state.players[0].battlefield[0];

        let legacy =
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::DeclareAttackers {
                player: PlayerId::P0,
                eligible: vec![object],
            }));
        let fast_legacy = core_policy_action_candidates_v5(&legacy, &state).unwrap_err();
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));
        let full_legacy = legal_action_candidates_v5(&legacy, &state).unwrap_err();
        assert_eq!(fast_legacy, full_legacy);
        assert_eq!(
            fast_legacy.0,
            "legacy aggregate combat semantic is forbidden on policy surface v5"
        );
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));

        let duplicate =
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::CastSpellOrPass {
                player: PlayerId::P0,
                castable_spells: vec![object, object],
                mana_abilities: Vec::new(),
                land_drops: Vec::new(),
                activatable_abilities: Vec::new(),
                plot_actions: Vec::new(),
            }));
        let fast_duplicate = core_policy_action_candidates_v5(&duplicate, &state).unwrap_err();
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));
        let full_duplicate = legal_action_candidates_v5(&duplicate, &state).unwrap_err();
        assert_eq!(fast_duplicate, full_duplicate);
        assert_eq!(
            fast_duplicate.0,
            "duplicate policy action semantic within one decision"
        );
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));

        let ambiguous_semantic = ActionSemanticV1::Ambiguous {
            reason: "audit_fixture".to_string(),
        };
        let ambiguous_core = [CorePolicyActionCandidateV1 {
            semantic: ambiguous_semantic.clone(),
            policy_action: crate::policy_surface_v5::PolicyActionV5::Surface(
                crate::surface_v2::SurfaceAction::Action(crate::engine::Action::Pass),
            ),
        }];
        let fast_ambiguous =
            validate_core_policy_action_candidates_v5(&ambiguous_core).unwrap_err();
        assert_eq!(test_policy_v5_materialization_calls(), (0, 0));
        let full_ambiguous = make_legal_action_v5(0, ambiguous_semantic, None).unwrap_err();
        assert_eq!(fast_ambiguous, full_ambiguous);
        assert_eq!(
            fast_ambiguous.0,
            "ambiguous legal action representation refused: audit_fixture"
        );
        assert_eq!(test_policy_v5_materialization_calls(), (0, 1));
    }

    fn prove_fast_actor_parity(deck_id: &str, seed: u64) {
        let episode_id = seed ^ 0xFA57_AC70_0000_0001;
        let deck_ids = [deck_id.to_string(), deck_id.to_string()];
        let mut full = RlEpisodeSessionV1::reset_with_decks_and_limits(
            episode_id,
            seed,
            4_096,
            524_288,
            deck_ids.clone(),
        )
        .unwrap();
        // FastActorResponseV1 deliberately has no serde/wire materialization.
        // JSONL bytes are therefore stabilized by an independent full-v5
        // replay, while the fast lane is compared below at exact core state,
        // candidate payload/order, counters, and terminal outcome.
        let mut full_jsonl_replay = RlEpisodeSessionV1::reset_with_decks_and_limits(
            episode_id,
            seed,
            4_096,
            524_288,
            deck_ids.clone(),
        )
        .unwrap();
        let mut fast = FastActorSessionV1::reset_with_decks_and_limits(
            episode_id, seed, 4_096, 524_288, deck_ids,
        )
        .unwrap();
        let mut clone_reference = fast.clone();
        let mut policy_rng = SplitMix64::seed(seed ^ 0xA66A_E6A7_E000_0005);
        let mut saw_attacker_inclusion = false;
        let mut saw_blocker_inclusion = false;

        for _ in 0..524_289 {
            let full_response = full.current_response();
            assert_eq!(
                serde_json::to_vec(&full_jsonl_replay.current_response()).unwrap(),
                serde_json::to_vec(&full_response).unwrap(),
                "{deck_id} independent full-v5 JSONL replay diverged before selection"
            );
            assert_eq!(
                fast.current_response(),
                clone_reference.current_response(),
                "{deck_id} in-place response diverged from clone reference"
            );
            assert_eq!(
                fast.state, clone_reference.state,
                "{deck_id} in-place state diverged from clone reference"
            );
            assert_eq!(
                fast.surface.harness_public_context(),
                clone_reference.surface.harness_public_context(),
                "{deck_id} in-place H2 context diverged from clone reference"
            );
            assert_eq!(
                fast.surface.privileged_scan_context().unwrap(),
                clone_reference.surface.privileged_scan_context().unwrap(),
                "{deck_id} in-place scan context diverged from clone reference"
            );
            assert_eq!(
                fast.privileged_core_environment_hash(),
                clone_reference.privileged_core_environment_hash(),
                "{deck_id} in-place core hash diverged from clone reference"
            );
            assert_eq!(full.state, fast.state, "{deck_id} state diverged");
            assert_eq!(
                full.surface.harness_public_context(),
                fast.surface.harness_public_context(),
                "{deck_id} public surface context diverged"
            );
            assert_eq!(
                full.surface.privileged_scan_context().unwrap(),
                fast.surface.privileged_scan_context().unwrap(),
                "{deck_id} private scan context diverged"
            );
            assert_eq!(full.environment_revision, fast.environment_revision);
            assert_eq!(full.policy_step_count, fast.policy_step_count);
            assert_eq!(full.physical_decision_count, fast.physical_decision_count);
            assert_eq!(full.diagnostic_state_hash(), fast.diagnostic_state_hash());
            assert_eq!(
                full.privileged_core_environment_hash(),
                fast.privileged_core_environment_hash(),
                "{deck_id} shared core environment hash diverged"
            );

            match (full_response, fast.current_response()) {
                (
                    RlSessionResponseV1::Terminal(full_terminal),
                    FastActorResponseV1::Terminal(fast_terminal),
                ) => {
                    assert_eq!(fast_terminal, full_terminal);
                    if deck_id == CANONICAL_RALLY_DECK_ID {
                        assert!(
                            saw_attacker_inclusion,
                            "Rally parity trace must exercise attacker decomposition"
                        );
                        assert!(
                            saw_blocker_inclusion,
                            "Rally parity trace must exercise blocker decomposition"
                        );
                    }
                    return;
                }
                (
                    RlSessionResponseV1::Decision(full_decision),
                    FastActorResponseV1::Decision(fast_decision),
                ) => {
                    assert_eq!(fast_decision.episode_id, full_decision.episode_id);
                    assert_eq!(fast_decision.step, full_decision.step);
                    assert_eq!(
                        fast_decision.physical_decision_id,
                        full_decision.physical_decision_id
                    );
                    assert_eq!(fast_decision.substep_index, full_decision.substep_index);
                    assert_eq!(fast_decision.substep_count, full_decision.substep_count);
                    assert_eq!(fast_decision.acting_player, full_decision.acting_player);
                    assert_eq!(
                        usize::try_from(fast_decision.legal_action_count).unwrap(),
                        full_decision.legal_actions.len()
                    );

                    let full_current = full.current.as_ref().unwrap();
                    let fast_current = fast.current.as_ref().unwrap();
                    assert_eq!(full_current.candidates.len(), fast_current.candidates.len());
                    for (wire, core) in full_current.candidates.iter().zip(&fast_current.candidates)
                    {
                        assert_eq!(wire.record.semantic, core.semantic);
                        assert_eq!(wire.policy_action, core.policy_action);
                    }
                    saw_attacker_inclusion |= fast_current.candidates.iter().any(|candidate| {
                        matches!(
                            candidate.semantic,
                            ActionSemanticV1::ChooseAttackerInclusion { .. }
                        )
                    });
                    saw_blocker_inclusion |= fast_current.candidates.iter().any(|candidate| {
                        matches!(
                            candidate.semantic,
                            ActionSemanticV1::ChooseBlockerInclusion { .. }
                        )
                    });
                    let selected_index = if matches!(
                        fast_current
                            .candidates
                            .get(1)
                            .map(|candidate| &candidate.semantic),
                        Some(
                            ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                                | ActionSemanticV1::ChooseBlockerInclusion { include: true, .. }
                        )
                    ) {
                        1usize
                    } else {
                        (policy_rng.next_u64() % fast_current.candidates.len() as u64) as usize
                    };
                    let selected = &full_decision.legal_actions[selected_index];
                    let full_next = full
                        .step(
                            episode_id,
                            full_decision.step,
                            selected.selected_index,
                            &selected.stable_id,
                        )
                        .unwrap();
                    let replay_next = full_jsonl_replay
                        .step(
                            episode_id,
                            full_decision.step,
                            selected.selected_index,
                            &selected.stable_id,
                        )
                        .unwrap();
                    assert_eq!(
                        serde_json::to_vec(&replay_next).unwrap(),
                        serde_json::to_vec(&full_next).unwrap(),
                        "{deck_id} independent full-v5 JSONL replay diverged after selection"
                    );
                    let fast_next = fast
                        .step(
                            episode_id,
                            fast_decision.step,
                            u32::try_from(selected_index).unwrap(),
                        )
                        .unwrap();
                    let reference_next = clone_reference
                        .step_clone_reference(
                            episode_id,
                            fast_decision.step,
                            u32::try_from(selected_index).unwrap(),
                        )
                        .unwrap();
                    assert_eq!(
                        fast_next, reference_next,
                        "{deck_id} in-place next response diverged from clone reference"
                    );
                }
                _ => panic!("{deck_id} full and fast terminal states diverged"),
            }
        }
        panic!("{deck_id} fast actor parity trace exceeded its policy-step bound");
    }

    #[test]
    fn fast_actor_matches_full_v5_core_and_full_replay_jsonl_is_stable() {
        prove_fast_actor_parity(CANONICAL_BURN_DECK_ID, 81_701);
        prove_fast_actor_parity(CANONICAL_RALLY_DECK_ID, 81_702);
    }

    fn reset_in_suppression_audit_mode(
        episode_id: u64,
        env_seed: u64,
        deck_id: &str,
        mode: SuppressionAuditMode,
    ) -> RlEpisodeSessionV1 {
        RlEpisodeSessionV1::reset_with_decks_and_limits_profiled_in_audit_mode(
            episode_id,
            env_seed,
            4_096,
            524_288,
            [deck_id.to_string(), deck_id.to_string()],
            None,
            mode,
        )
        .unwrap()
    }

    fn prove_suppression_audit_mode_equivalence(deck_id: &str, seed: u64) {
        let episode_id = seed ^ 0xA11D_1700_0000_0001;
        let mut sessions = [
            reset_in_suppression_audit_mode(episode_id, seed, deck_id, SuppressionAuditMode::Full),
            reset_in_suppression_audit_mode(
                episode_id,
                seed,
                deck_id,
                SuppressionAuditMode::CountOnly,
            ),
            reset_in_suppression_audit_mode(episode_id, seed, deck_id, SuppressionAuditMode::Off),
        ];
        let mut policy_rng = SplitMix64::seed(seed ^ 0x50A1_CE00_0000_0005);
        let mut reached_terminal = false;
        let mut policy_steps_seen = 0u64;
        let mut midrun_hash_checkpoint_seen = false;

        for _ in 0..524_289 {
            let reference = sessions[0].current_response();
            let reference_bytes = serde_json::to_vec(&reference).unwrap();
            let reference_context = sessions[0].surface.harness_public_context();
            let compare_privileged_hashes = policy_steps_seen == 0
                || policy_steps_seen == 32
                || matches!(&reference, RlSessionResponseV1::Terminal(_));
            let reference_hashes = compare_privileged_hashes.then(|| {
                (
                    sessions[0].diagnostic_state_hash(),
                    sessions[0].privileged_environment_hash(),
                )
            });
            midrun_hash_checkpoint_seen |= policy_steps_seen == 32;

            for (index, session) in sessions.iter().enumerate().skip(1) {
                assert_eq!(
                    serde_json::to_vec(&session.current_response()).unwrap(),
                    reference_bytes,
                    "{deck_id} profile-off session response bytes diverged in audit mode {index}"
                );
                assert_eq!(
                    session.surface.harness_public_context(),
                    reference_context,
                    "{deck_id} public H2 context diverged in audit mode {index}"
                );
                if let Some((reference_state_hash, reference_environment_hash)) = reference_hashes {
                    assert_eq!(
                        session.diagnostic_state_hash(),
                        reference_state_hash,
                        "{deck_id} diagnostic state hash diverged in audit mode {index}"
                    );
                    assert_eq!(
                        session.privileged_environment_hash(),
                        reference_environment_hash,
                        "{deck_id} stable action/environment binding diverged in audit mode {index}"
                    );
                }
            }

            let RlSessionResponseV1::Decision(decision) = reference else {
                reached_terminal = true;
                break;
            };
            assert!(!decision.legal_actions.is_empty());
            let selected_index =
                (policy_rng.next_u64() % decision.legal_actions.len() as u64) as usize;
            let selected = &decision.legal_actions[selected_index];
            let expected_next = sessions[0]
                .step(
                    episode_id,
                    decision.step,
                    selected.selected_index,
                    &selected.stable_id,
                )
                .unwrap();
            let expected_next_bytes = serde_json::to_vec(&expected_next).unwrap();
            for (index, session) in sessions.iter_mut().enumerate().skip(1) {
                let next = session
                    .step(
                        episode_id,
                        decision.step,
                        selected.selected_index,
                        &selected.stable_id,
                    )
                    .unwrap();
                assert_eq!(
                    serde_json::to_vec(&next).unwrap(),
                    expected_next_bytes,
                    "{deck_id} decisions, actions, or stable ids diverged in audit mode {index}"
                );
            }
            policy_steps_seen = policy_steps_seen.saturating_add(1);
        }
        assert!(
            reached_terminal,
            "{deck_id} fixture did not reach a terminal response"
        );
        assert!(
            midrun_hash_checkpoint_seen,
            "{deck_id} fixture did not reach its privileged mid-run hash checkpoint"
        );

        let full_surface = sessions[0].surface.harness_surface();
        let count_surface = sessions[1].surface.harness_surface();
        let off_surface = sessions[2].surface.harness_surface();
        assert_eq!(
            count_surface.suppression_counts(),
            full_surface.suppression_counts(),
            "{deck_id} CountOnly reason counts must exactly match Full records"
        );
        assert_eq!(
            full_surface.suppression_counts().total(),
            full_surface.suppressions().len() as u64
        );
        assert!(
            !full_surface.suppressions().is_empty(),
            "{deck_id} fixture must exercise at least one suppression"
        );
        assert!(count_surface.suppressions().is_empty());
        assert_eq!(off_surface.suppression_counts().total(), 0);
        assert!(off_surface.suppressions().is_empty());
        assert_eq!(
            sessions[1].diagnostic_state_hash(),
            sessions[0].diagnostic_state_hash(),
            "{deck_id} terminal state hash differs for CountOnly"
        );
        assert_eq!(
            sessions[2].diagnostic_state_hash(),
            sessions[0].diagnostic_state_hash(),
            "{deck_id} terminal state hash differs for Off"
        );
    }

    #[test]
    fn suppression_audit_modes_are_byte_and_semantics_inert_for_burn_and_rally() {
        prove_suppression_audit_mode_equivalence(CANONICAL_BURN_DECK_ID, 71_501);
        prove_suppression_audit_mode_equivalence(CANONICAL_RALLY_DECK_ID, 71_502);
    }

    #[test]
    fn suppression_audit_mode_does_not_change_integrity_error_precedence() {
        for mode in [
            SuppressionAuditMode::Full,
            SuppressionAuditMode::CountOnly,
            SuppressionAuditMode::Off,
        ] {
            let episode_id = 71_503;
            let mut session =
                reset_in_suppression_audit_mode(episode_id, 19_991, CANONICAL_RALLY_DECK_ID, mode);
            let RlSessionResponseV1::Decision(decision) = session.current_response() else {
                panic!("Rally fixture must begin with a decision");
            };
            session.current.as_mut().unwrap().environment_revision += 1;
            let error = session
                .step(episode_id, decision.step, u32::MAX, "unknown-action")
                .unwrap_err();
            assert_eq!(error.code, RlSessionErrorCode::StaleEnvironmentBinding);
        }
    }
}
