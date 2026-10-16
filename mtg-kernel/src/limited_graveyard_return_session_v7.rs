//! Distinct schema-7 actor-facing playback over the sealed GraveyardReturn V61 owner.
//! This is a trusted two-seat host interface, not a remote-player authorization
//! service. Only the current actor's canonical safe DTO is returned.
use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::human_bo3_v1::{
    project_limited_graveyard_return_v7, HumanActionReceiptV1, LimitedGraveyardReturnDecisionV7,
};
use crate::limited_session_v1::{CustomDeckV1, MAX_LIMITED_LINE_BYTES_V1};
use crate::rl::PlayerSeatV1;

use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, GraveyardReturnDecisionV61,
    GraveyardReturnSessionV61, GraveyardReturnSnapshotV61,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const PROTOCOL: &str = "kernel_limited_graveyard_return_jsonl";
pub const SCHEMA_VERSION: u32 = 7;
const MAX_SNAPSHOTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "request_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LimitedGraveyardReturnRequestV7 {
    Reset {
        schema_version: u32,
        request_id: String,
        expected_card_db_hash: u64,
        episode_id: u64,
        env_seed: u64,
        starting_player: PlayerSeatV1,
        decks: [CustomDeckV1; 2],
        max_physical_decisions: u64,
        max_policy_steps: u64,
    },
    Current {
        schema_version: u32,
        request_id: String,
    },
    Step {
        schema_version: u32,
        request_id: String,
        episode_id: u64,
        authority_generation: u64,
        actor: PlayerSeatV1,
        prompt_seq: u64,
        action_index: u32,
    },
    Snapshot {
        schema_version: u32,
        request_id: String,
    },
    Restore {
        schema_version: u32,
        request_id: String,
        snapshot_id: u64,
    },
}
impl LimitedGraveyardReturnRequestV7 {
    fn header(&self) -> (u32, &str) {
        match self {
            Self::Reset {
                schema_version,
                request_id,
                ..
            }
            | Self::Current {
                schema_version,
                request_id,
            }
            | Self::Step {
                schema_version,
                request_id,
                ..
            }
            | Self::Snapshot {
                schema_version,
                request_id,
            }
            | Self::Restore {
                schema_version,
                request_id,
                ..
            } => (*schema_version, request_id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state_type", rename_all = "snake_case")]
pub enum LimitedGraveyardReturnStateV7 {
    Decision {
        episode_id: u64,
        authority_generation: u64,
        decision: Box<LimitedGraveyardReturnDecisionV7>,
    },
    Terminal {
        episode_id: u64,
        outcome: crate::rl::TerminalOutcomeV1,
        classification: crate::rl::TerminalClassificationV1,
        code: crate::rl::TerminalSafeCodeV2,
        winner: Option<PlayerSeatV1>,
        policy_steps: u64,
        physical_decisions: u64,
    },
    /// The owning engine state is retained. No fallback action is applied.
    Unsupported { episode_id: u64, code: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "response_type", rename_all = "snake_case")]
pub enum LimitedGraveyardReturnBodyV7 {
    State {
        state: Box<LimitedGraveyardReturnStateV7>,
        #[serde(skip_serializing_if = "Option::is_none")]
        receipt: Option<HumanActionReceiptV1>,
        #[serde(skip_serializing_if = "Option::is_none")]
        snapshot_id: Option<u64>,
    },
    Error {
        code: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LimitedGraveyardReturnReplyV7 {
    pub protocol: String,
    pub schema_version: u32,
    pub request_id: Option<String>,
    pub kernel_version: String,
    pub card_db_hash: u64,
    pub card_count: usize,
    pub producer_sha256: String,
    pub priority_mode: String,
    pub combat_rules: String,
    pub mulligan_rules: String,
    #[serde(flatten)]
    pub body: LimitedGraveyardReturnBodyV7,
}

/// Content identity is provenance only. Executable authority never crosses JSON.
pub fn producer_sha256_v7() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [
            PROTOCOL,
            include_str!("limited_graveyard_return_session_v7.rs"),
            include_str!("bin/kernel_limited_graveyard_return_env_v7.rs"),
            include_str!("human_bo3_v1/limited_graveyard_return_v7.rs"),
            include_str!("human_bo3_v1/graveyard_return_v61.rs"),
            include_str!("human_bo3_v1/next_five_v59.rs"),
            include_str!("human_bo3_v1/prepared_v57.rs"),
            include_str!("human_bo3_v1/ravenous_v51.rs"),
            include_str!("human_bo3_v1/ravenous_v51/exact_state.rs"),
            include_str!("human_bo3_v1/ravenous_v51/ravenous_facts.rs"),
            include_str!("human_bo3_v1/labels.rs"),
            include_str!("human_bo3_v1/visible.rs"),
            include_str!("human_bo3_v1/limited_common_v1.rs"),
            include_str!("rl_session/graveyard_return_v61.rs"),
            include_str!("rl_session/limited_common_v1.rs"),
            include_str!("limited_session_v1.rs"),
        ] {
            hash.update([0]);
            hash.update(source.as_bytes());
        }
        hash.update(crate::graveyard_return_candidates_v61::producer_sha256().as_bytes());
        hash.update(crate::limited_next_five_session_v6::producer_sha256_v6().as_bytes());
        format!("{:x}", hash.finalize())
    })
    .clone()
}

// None of the following authority/continuation structures have Debug or serde.
struct Active {
    core: GraveyardReturnSessionV61,
    owner: Arc<()>,
}
struct Bound {
    owner: Arc<()>,
    generation: u64,
    expected: FastActorDecisionV1,
    decision: GraveyardReturnDecisionV61,
    visible: LimitedGraveyardReturnDecisionV7,
    indexes: Vec<u32>,
}
struct Saved {
    owner: Arc<()>,
    core: GraveyardReturnSnapshotV61,
}
struct Accepted {
    owner: Arc<()>,
    generation: u64,
    episode_id: u64,
    actor: PlayerSeatV1,
    receipt: HumanActionReceiptV1,
}

#[derive(Default)]
pub struct LimitedGraveyardReturnJsonlServerV7 {
    active: Option<Active>,
    generation: u64,
    next_prompt: u64,
    pending: Option<Bound>,
    accepted: Option<Accepted>,
    snapshots: BTreeMap<u64, Saved>,
    next_snapshot: u64,
    last_exchange: Option<(
        LimitedGraveyardReturnRequestV7,
        LimitedGraveyardReturnReplyV7,
        u64,
    )>,
    timings: Option<GraveyardReturnTimingsV7>,
}

#[derive(Default, Serialize)]
struct GraveyardReturnTimingsV7 {
    apply_calls: u64,
    apply_seconds: f64,
    projection_seconds: f64,
    reply_seconds: f64,
}
impl LimitedGraveyardReturnJsonlServerV7 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process diagnostics only. Timings never enter replies, game state,
    /// snapshots, action selection or executable authority.
    pub fn enable_timings_v7(&mut self) {
        self.timings = Some(GraveyardReturnTimingsV7::default());
    }

    /// Phase samples exclude parsing, JSON encoding, I/O and early retries.
    /// Their sum is not the process's total elapsed time.
    pub fn timing_report_v7(&self) -> Option<String> {
        self.timings.as_ref().map(|timings| {
            serde_json::to_string(timings).expect("bounded process timings serialize")
        })
    }

    fn reply(
        &self,
        request: Option<String>,
        body: LimitedGraveyardReturnBodyV7,
    ) -> LimitedGraveyardReturnReplyV7 {
        LimitedGraveyardReturnReplyV7 {
            protocol: PROTOCOL.into(),
            schema_version: SCHEMA_VERSION,
            request_id: request,
            kernel_version: crate::KERNEL_VERSION.into(),
            card_db_hash: KERNEL_CARDDB_HASH,
            card_count: CARD_DEFS.len(),
            producer_sha256: producer_sha256_v7(),
            priority_mode: "engine_windows_v1".into(),
            combat_rules: "foundations_v1".into(),
            mulligan_rules: "london_v1".into(),
            body,
        }
    }
    fn error(&self, request: Option<String>, code: &str) -> LimitedGraveyardReturnReplyV7 {
        self.reply(
            request,
            LimitedGraveyardReturnBodyV7::Error { code: code.into() },
        )
    }
    pub fn handle_line(&mut self, line: &str) -> String {
        let request = if line.len() <= MAX_LIMITED_LINE_BYTES_V1 {
            crate::rl::parse_strict_json_value(line)
                .ok()
                .and_then(|value| {
                    serde_json::from_value::<LimitedGraveyardReturnRequestV7>(value).ok()
                })
        } else {
            None
        };
        let reply = match request {
            Some(request) => self.handle_request(request),
            None => self.error(None, "invalid_request"),
        };
        serde_json::to_string(&reply).expect("safe Limited reply serializes")
    }
    pub fn handle_request(
        &mut self,
        request: LimitedGraveyardReturnRequestV7,
    ) -> LimitedGraveyardReturnReplyV7 {
        let (version, id) = request.header();
        let id = id.to_owned();
        if version != SCHEMA_VERSION || id.is_empty() || id.len() > 256 {
            return self.error(Some(id), "invalid_header");
        }
        if let Some((previous, reply, generation)) = &self.last_exchange {
            if previous.header().1 == id {
                return if previous == &request && *generation == self.generation {
                    reply.clone()
                } else {
                    self.error(Some(id), "conflicting_retry")
                };
            }
        }
        let apply_started = self.timings.as_ref().map(|_| std::time::Instant::now());
        let result = self.apply(&request);
        if let (Some(timings), Some(started)) = (&mut self.timings, apply_started) {
            timings.apply_calls += 1;
            timings.apply_seconds += started.elapsed().as_secs_f64();
        }
        let mut reply_started = self.timings.as_ref().map(|_| std::time::Instant::now());
        let reply = match result {
            Ok((receipt, snapshot_id)) => {
                let projection_started = self.timings.as_ref().map(|_| std::time::Instant::now());
                let state = self.current_state();
                if let (Some(timings), Some(started)) = (&mut self.timings, projection_started) {
                    timings.projection_seconds += started.elapsed().as_secs_f64();
                }
                reply_started = self.timings.as_ref().map(|_| std::time::Instant::now());
                self.reply(
                    Some(id),
                    LimitedGraveyardReturnBodyV7::State {
                        state: Box::new(state),
                        receipt,
                        snapshot_id,
                    },
                )
            }
            Err(code) => self.error(Some(id), code),
        };
        self.last_exchange = Some((request, reply.clone(), self.generation));
        if let (Some(timings), Some(started)) = (&mut self.timings, reply_started) {
            timings.reply_seconds += started.elapsed().as_secs_f64();
        }
        reply
    }
    fn apply(
        &mut self,
        request: &LimitedGraveyardReturnRequestV7,
    ) -> Result<(Option<HumanActionReceiptV1>, Option<u64>), &'static str> {
        match request {
            LimitedGraveyardReturnRequestV7::Reset {
                expected_card_db_hash,
                episode_id,
                env_seed,
                starting_player,
                decks,
                max_physical_decisions,
                max_policy_steps,
                ..
            } => {
                if *expected_card_db_hash != KERNEL_CARDDB_HASH {
                    return Err("catalog_mismatch");
                }
                let next = self
                    .generation
                    .checked_add(1)
                    .ok_or("authority_exhausted")?;
                let boards = [
                    decks[0]
                        .resolve_graveyard_return_v61()
                        .map_err(|_| "unsupported_deck")?,
                    decks[1]
                        .resolve_graveyard_return_v61()
                        .map_err(|_| "unsupported_deck")?,
                ];
                let identities = boards.each_ref().map(|deck| {
                    let mut hash = Sha256::new();
                    hash.update(b"kernel_limited_common_deck/v1");
                    hash.update(KERNEL_CARDDB_HASH.to_le_bytes());
                    for card in deck {
                        hash.update(card.to_le_bytes());
                    }
                    format!("limited-common-v1:{:x}", hash.finalize())
                });
                let core = GraveyardReturnSessionV61::reset_explicit_mainboards_v61(
                    *episode_id,
                    *env_seed,
                    (*max_physical_decisions, *max_policy_steps),
                    identities,
                    boards,
                    match starting_player {
                        PlayerSeatV1::P0 => crate::ids::PlayerId::P0,
                        PlayerSeatV1::P1 => crate::ids::PlayerId::P1,
                    },
                )
                .map_err(|_| "unsupported_reset")?;
                self.active = Some(Active {
                    core,
                    owner: Arc::new(()),
                });
                self.generation = next;
                self.pending = None;
                self.snapshots.clear();
                Ok((None, None))
            }
            LimitedGraveyardReturnRequestV7::Current { .. } => {
                self.active.as_ref().ok_or("no_session")?;
                Ok((None, None))
            }
            LimitedGraveyardReturnRequestV7::Step {
                episode_id,
                authority_generation,
                actor,
                prompt_seq,
                action_index,
                ..
            } => {
                let active = self.active.as_mut().ok_or("no_session")?;
                if let Some(accepted) = &self.accepted {
                    if *prompt_seq == accepted.receipt.prompt_seq {
                        if *authority_generation != self.generation
                            || accepted.generation != self.generation
                            || *episode_id != accepted.episode_id
                            || *actor != accepted.actor
                            || !Arc::ptr_eq(&accepted.owner, &active.owner)
                        {
                            return Err("stale_decision");
                        }
                        if *action_index != accepted.receipt.action_index {
                            return Err("conflicting_retry");
                        }
                        return Ok((Some(accepted.receipt.clone()), None));
                    }
                }
                let bound = self.pending.as_ref().ok_or("no_executable_prompt")?;
                if *authority_generation != self.generation
                    || bound.generation != self.generation
                    || !Arc::ptr_eq(&active.owner, &bound.owner)
                    || *episode_id != bound.expected.episode_id
                    || *actor != bound.expected.acting_player
                    || *prompt_seq != bound.visible.prompt_seq
                {
                    return Err("stale_decision");
                }
                let index = *bound
                    .indexes
                    .get(*action_index as usize)
                    .ok_or("invalid_action")?;
                active
                    .core
                    .validate_decision_v61(&bound.decision)
                    .map_err(|_| "changed_binding")?;
                let (visible, indexes) = project_limited_graveyard_return_v7(
                    &bound.decision,
                    *actor,
                    bound.visible.prompt_seq,
                )
                .map_err(|_| "changed_safe_prompt")?;
                if visible != bound.visible || indexes != bound.indexes {
                    return Err("changed_safe_prompt");
                }
                active.core.step_with_current_check(
                    &bound.decision,
                    index,
                    |_, _| {
                        let (visible, indexes) = project_limited_graveyard_return_v7(
                            &bound.decision,
                            *actor,
                            bound.visible.prompt_seq,
                        )
                        .map_err(|_| "changed_safe_prompt")?;
                        if visible != bound.visible || indexes != bound.indexes {
                            return Err("changed_safe_prompt");
                        }
                        Ok(())
                    },
                    |_| "changed_binding",
                    |_| "execution_failed",
                )?;
                self.pending = None;
                let receipt = HumanActionReceiptV1 {
                    prompt_seq: *prompt_seq,
                    action_index: *action_index,
                };
                self.accepted = Some(Accepted {
                    owner: active.owner.clone(),
                    generation: self.generation,
                    episode_id: *episode_id,
                    actor: *actor,
                    receipt: receipt.clone(),
                });
                Ok((Some(receipt), None))
            }
            LimitedGraveyardReturnRequestV7::Snapshot { .. } => {
                let active = self.active.as_ref().ok_or("no_session")?;
                if self.snapshots.len() >= MAX_SNAPSHOTS {
                    return Err("snapshot_limit");
                }
                let id = self
                    .next_snapshot
                    .checked_add(1)
                    .ok_or("snapshot_exhausted")?;
                self.snapshots.insert(
                    id,
                    Saved {
                        owner: active.owner.clone(),
                        core: active.core.snapshot_v61(),
                    },
                );
                self.next_snapshot = id;
                Ok((None, Some(id)))
            }
            LimitedGraveyardReturnRequestV7::Restore { snapshot_id, .. } => {
                let saved = self.snapshots.get(snapshot_id).ok_or("unknown_snapshot")?;
                let active = self.active.as_mut().ok_or("no_session")?;
                if !Arc::ptr_eq(&saved.owner, &active.owner) {
                    return Err("foreign_snapshot");
                }
                let next = self
                    .generation
                    .checked_add(1)
                    .ok_or("authority_exhausted")?;
                active
                    .core
                    .restore_v61(&saved.core)
                    .map_err(|_| "invalid_snapshot")?;
                self.generation = next;
                self.pending = None;
                Ok((None, None))
            }
        }
    }
    fn current_state(&mut self) -> LimitedGraveyardReturnStateV7 {
        let Some(active) = self.active.as_ref() else {
            return LimitedGraveyardReturnStateV7::Unsupported {
                episode_id: 0,
                code: "no_session".into(),
            };
        };
        match active.core.response() {
            FastActorResponseV1::Terminal(terminal) => {
                self.pending = None;
                LimitedGraveyardReturnStateV7::Terminal {
                    episode_id: terminal.episode_id,
                    outcome: terminal.terminal_outcome,
                    classification: terminal.terminal_classification,
                    code: terminal.terminal_code,
                    winner: terminal.winner,
                    policy_steps: terminal.policy_step_count,
                    physical_decisions: terminal.physical_decision_count,
                }
            }
            FastActorResponseV1::Decision(expected) => {
                let Ok(decision) = active.core.decision_v61() else {
                    self.pending = None;
                    return LimitedGraveyardReturnStateV7::Unsupported {
                        episode_id: expected.episode_id,
                        code: "unsupported_graveyard_return_input".into(),
                    };
                };
                if decision.expected() != expected {
                    self.pending = None;
                    return LimitedGraveyardReturnStateV7::Unsupported {
                        episode_id: expected.episode_id,
                        code: "changed_graveyard_return_header".into(),
                    };
                }
                if let Some(bound) = &self.pending {
                    if bound.generation == self.generation
                        && Arc::ptr_eq(&bound.owner, &active.owner)
                        && bound.expected == expected
                        && active.core.validate_decision_v61(&bound.decision).is_ok()
                    {
                        if let Ok((visible, indexes)) = project_limited_graveyard_return_v7(
                            &decision,
                            expected.acting_player,
                            bound.visible.prompt_seq,
                        ) {
                            if visible == bound.visible && indexes == bound.indexes {
                                return LimitedGraveyardReturnStateV7::Decision {
                                    episode_id: expected.episode_id,
                                    authority_generation: self.generation,
                                    decision: Box::new(visible),
                                };
                            }
                        }
                    }
                }
                let Some(next) = self.next_prompt.checked_add(1) else {
                    self.pending = None;
                    return LimitedGraveyardReturnStateV7::Unsupported {
                        episode_id: expected.episode_id,
                        code: "prompt_exhausted".into(),
                    };
                };
                let Ok((visible, indexes)) =
                    project_limited_graveyard_return_v7(&decision, expected.acting_player, next)
                else {
                    self.pending = None;
                    return LimitedGraveyardReturnStateV7::Unsupported {
                        episode_id: expected.episode_id,
                        code: "unsupported_safe_prompt".into(),
                    };
                };
                self.next_prompt = next;
                self.pending = Some(Bound {
                    owner: active.owner.clone(),
                    generation: self.generation,
                    expected,
                    decision,
                    visible: visible.clone(),
                    indexes,
                });
                LimitedGraveyardReturnStateV7::Decision {
                    episode_id: expected.episode_id,
                    authority_generation: self.generation,
                    decision: Box::new(visible),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
