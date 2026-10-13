//! Opt-in actor-facing Limited playback over the complete common V40 binding.
//! This is a trusted two-seat host interface, not a remote-player authorization
//! service. Only the current actor's canonical safe DTO is returned.
use crate::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::human_bo3_v1::{
    project_limited_common_v1, HumanActionReceiptV1, LimitedCommonDecisionV1,
};
use crate::limited_session_v1::{CustomDeckV1, MAX_LIMITED_LINE_BYTES_V1};
use crate::rl::PlayerSeatV1;
use crate::rl_session::akroma_v40::AkromaDecisionBindingV40;
use crate::rl_session::{
    FastActorDecisionV1, FastActorResponseV1, FastActorSessionSnapshotV1, FastActorSessionV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const PROTOCOL: &str = "kernel_limited_common_jsonl";
pub const SCHEMA_VERSION: u32 = 1;
const MAX_SNAPSHOTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "request_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LimitedCommonRequestV1 {
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
impl LimitedCommonRequestV1 {
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
pub enum LimitedCommonStateV1 {
    Decision {
        episode_id: u64,
        authority_generation: u64,
        decision: Box<LimitedCommonDecisionV1>,
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
pub enum LimitedCommonBodyV1 {
    State {
        state: Box<LimitedCommonStateV1>,
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
pub struct LimitedCommonReplyV1 {
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
    pub body: LimitedCommonBodyV1,
}

/// Content identity is provenance only. Executable authority never crosses JSON.
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        PROTOCOL,
        include_str!("limited_common_session_v1.rs"),
        include_str!("rl_session/limited_common_v1.rs"),
        include_str!("human_bo3_v1/limited_common_v1.rs"),
        include_str!("human_bo3_v1/akroma_v40.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("london_mulligan_v1.rs"),
        include_str!("combat_damage_v1.rs"),
        include_str!("policy_surface_v5.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.update(crate::akroma_public_v1::producer_sha256_v1().as_bytes());
    format!("{:x}", hash.finalize())
}

// None of the following authority/continuation structures have Debug or serde.
struct Active {
    core: FastActorSessionV1,
    owner: Arc<()>,
}
struct Bound {
    owner: Arc<()>,
    generation: u64,
    expected: FastActorDecisionV1,
    binding: AkromaDecisionBindingV40,
    visible: LimitedCommonDecisionV1,
    indexes: Vec<u32>,
}
struct Saved {
    owner: Arc<()>,
    core: FastActorSessionSnapshotV1,
}
struct Accepted {
    owner: Arc<()>,
    generation: u64,
    episode_id: u64,
    actor: PlayerSeatV1,
    receipt: HumanActionReceiptV1,
}

#[derive(Default)]
pub struct LimitedCommonJsonlServerV1 {
    active: Option<Active>,
    generation: u64,
    next_prompt: u64,
    pending: Option<Bound>,
    accepted: Option<Accepted>,
    snapshots: BTreeMap<u64, Saved>,
    next_snapshot: u64,
    last_exchange: Option<(LimitedCommonRequestV1, LimitedCommonReplyV1, u64)>,
}
impl LimitedCommonJsonlServerV1 {
    pub fn new() -> Self {
        Self::default()
    }

    fn reply(&self, request: Option<String>, body: LimitedCommonBodyV1) -> LimitedCommonReplyV1 {
        LimitedCommonReplyV1 {
            protocol: PROTOCOL.into(),
            schema_version: SCHEMA_VERSION,
            request_id: request,
            kernel_version: crate::KERNEL_VERSION.into(),
            card_db_hash: KERNEL_CARDDB_HASH,
            card_count: CARD_DEFS.len(),
            producer_sha256: producer_sha256_v1(),
            priority_mode: "engine_windows_v1".into(),
            combat_rules: "foundations_v1".into(),
            mulligan_rules: "london_v1".into(),
            body,
        }
    }
    fn error(&self, request: Option<String>, code: &str) -> LimitedCommonReplyV1 {
        self.reply(request, LimitedCommonBodyV1::Error { code: code.into() })
    }
    pub fn handle_line(&mut self, line: &str) -> String {
        let request = if line.len() <= MAX_LIMITED_LINE_BYTES_V1 {
            crate::rl::parse_strict_json_value(line)
                .ok()
                .and_then(|value| serde_json::from_value::<LimitedCommonRequestV1>(value).ok())
        } else {
            None
        };
        let reply = match request {
            Some(request) => self.handle_request(request),
            None => self.error(None, "invalid_request"),
        };
        serde_json::to_string(&reply).expect("safe Limited reply serializes")
    }
    pub fn handle_request(&mut self, request: LimitedCommonRequestV1) -> LimitedCommonReplyV1 {
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
        let result = self.apply(&request);
        let reply = match result {
            Ok((receipt, snapshot_id)) => {
                let state = self.current_state();
                self.reply(
                    Some(id),
                    LimitedCommonBodyV1::State {
                        state: Box::new(state),
                        receipt,
                        snapshot_id,
                    },
                )
            }
            Err(code) => self.error(Some(id), code),
        };
        self.last_exchange = Some((request, reply.clone(), self.generation));
        reply
    }
    fn apply(
        &mut self,
        request: &LimitedCommonRequestV1,
    ) -> Result<(Option<HumanActionReceiptV1>, Option<u64>), &'static str> {
        match request {
            LimitedCommonRequestV1::Reset {
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
                    decks[0].resolve().map_err(|_| "unsupported_deck")?,
                    decks[1].resolve().map_err(|_| "unsupported_deck")?,
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
                let core = FastActorSessionV1::reset_limited_common_v1(
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
            LimitedCommonRequestV1::Current { .. } => {
                self.active.as_ref().ok_or("no_session")?;
                Ok((None, None))
            }
            LimitedCommonRequestV1::Step {
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
                    .akroma_validate_bound_decision_v40(bound.expected, *actor, &bound.binding)
                    .map_err(|_| "changed_binding")?;
                active
                    .core
                    .step(bound.expected.episode_id, bound.expected.step, index)
                    .map_err(|_| "execution_failed")?;
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
            LimitedCommonRequestV1::Snapshot { .. } => {
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
                        core: active.core.snapshot_v1(),
                    },
                );
                self.next_snapshot = id;
                Ok((None, Some(id)))
            }
            LimitedCommonRequestV1::Restore { snapshot_id, .. } => {
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
                    .akroma_restore_v40(&saved.core)
                    .map_err(|_| "invalid_snapshot")?;
                self.generation = next;
                self.pending = None;
                Ok((None, None))
            }
        }
    }
    fn current_state(&mut self) -> LimitedCommonStateV1 {
        let Some(active) = self.active.as_ref() else {
            return LimitedCommonStateV1::Unsupported {
                episode_id: 0,
                code: "no_session".into(),
            };
        };
        match active.core.current_response() {
            FastActorResponseV1::Terminal(terminal) => {
                self.pending = None;
                LimitedCommonStateV1::Terminal {
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
                let prepared = active
                    .core
                    .akroma_current_decision_input_v40(expected, expected.acting_player);
                let Ok((observation, actions, binding)) = prepared else {
                    self.pending = None;
                    return LimitedCommonStateV1::Unsupported {
                        episode_id: expected.episode_id,
                        code: "unsupported_common_input".into(),
                    };
                };
                if let Some(bound) = &self.pending {
                    if bound.generation == self.generation
                        && Arc::ptr_eq(&bound.owner, &active.owner)
                        && bound.expected == expected
                        && bound.binding == binding
                    {
                        return LimitedCommonStateV1::Decision {
                            episode_id: expected.episode_id,
                            authority_generation: self.generation,
                            decision: Box::new(bound.visible.clone()),
                        };
                    }
                }
                let Some(next) = self.next_prompt.checked_add(1) else {
                    self.pending = None;
                    return LimitedCommonStateV1::Unsupported {
                        episode_id: expected.episode_id,
                        code: "prompt_exhausted".into(),
                    };
                };
                let Ok((visible, indexes)) = project_limited_common_v1(
                    &observation,
                    &actions,
                    expected.acting_player,
                    &binding.families,
                    next,
                ) else {
                    self.pending = None;
                    return LimitedCommonStateV1::Unsupported {
                        episode_id: expected.episode_id,
                        code: "unsupported_safe_prompt".into(),
                    };
                };
                self.next_prompt = next;
                self.pending = Some(Bound {
                    owner: active.owner.clone(),
                    generation: self.generation,
                    expected,
                    binding,
                    visible: visible.clone(),
                    indexes,
                });
                LimitedCommonStateV1::Decision {
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
