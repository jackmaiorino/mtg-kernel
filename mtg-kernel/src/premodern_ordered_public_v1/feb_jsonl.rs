//! Actor-facing JSONL transport for the exact frozen FEB owner.
use super::feb::{
    FebCurrentV1, FebDecisionV1, FebHumanActionV1, FebHumanViewV1, FebNumericViewV1, FebSessionV1,
    FebSnapshotV1, PROFILE_V1,
};
use crate::{
    card_def::KERNEL_CARDDB_HASH, current_mixed_policy_v1::NumericTree,
    limited_session_v1::MAX_LIMITED_LINE_BYTES_V1, rl::PlayerSeatV1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{hash_map::RandomState, BTreeMap},
    hash::BuildHasher,
};

pub const PROTOCOL: &str = "kernel_premodern_feb_jsonl";
pub const SCHEMA_VERSION: u32 = 1;
const MAX_SNAPSHOTS: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Prompt {
    episode_id: u64,
    authority_generation: u64,
    actor: PlayerSeatV1,
    prompt_seq: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Submission {
    Numeric,
    Human,
}

#[derive(PartialEq, Eq, Deserialize)]
#[serde(tag = "request_type", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Reset {
        schema_version: u32,
        request_id: String,
        expected_card_db_hash: u64,
        expected_profile: String,
        episode_id: u64,
        seed: u64,
        starting_player: PlayerSeatV1,
    },
    Current {
        schema_version: u32,
        request_id: String,
    },
    Step {
        schema_version: u32,
        request_id: String,
        prompt: Prompt,
        native_ordinal: u32,
        submission: Submission,
    },
    Snapshot {
        schema_version: u32,
        request_id: String,
    },
    Restore {
        schema_version: u32,
        request_id: String,
        snapshot_id: String,
    },
}

impl Request {
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

#[derive(Clone, Copy, Serialize)]
struct Receipt {
    prompt: Prompt,
    native_ordinal: u32,
    submission: Submission,
}

struct Bound {
    prompt: Prompt,
    numeric: Box<FebNumericViewV1>,
    human: FebHumanViewV1,
    decision: FebDecisionV1,
}

enum Projection {
    Decision(Box<Bound>),
    Terminal { winner: Option<PlayerSeatV1> },
    NativeHalt,
    Unsupported(&'static str),
}

impl Projection {
    fn read(core: &FebSessionV1, episode_id: u64, authority_generation: u64) -> Self {
        let actor = core.actor();
        match core.current(actor.unwrap_or(PlayerSeatV1::P0)) {
            Ok(FebCurrentV1::Decision {
                numeric,
                decision: Some(decision),
            }) if actor == Some(numeric.actor) => {
                let Ok(Some(human)) = core.human_current(numeric.actor) else {
                    return Self::Unsupported("unsupported_human_projection");
                };
                if human.actor != numeric.actor
                    || human.step != numeric.step
                    || human.profile != numeric.profile
                    || human.actions.is_empty()
                    || human.actions.len() != numeric.actions.len()
                    || !human
                        .actions
                        .iter()
                        .enumerate()
                        .all(|(index, action)| match action {
                            FebHumanActionV1::Supported { native_ordinal, .. }
                            | FebHumanActionV1::Unsupported { native_ordinal, .. } => {
                                *native_ordinal == index
                            }
                        })
                {
                    return Self::Unsupported("inconsistent_projection");
                }
                Self::Decision(Box::new(Bound {
                    prompt: Prompt {
                        episode_id,
                        authority_generation,
                        actor: numeric.actor,
                        prompt_seq: numeric.step,
                    },
                    numeric,
                    human,
                    decision,
                }))
            }
            Ok(FebCurrentV1::GameOver { winner }) => Self::Terminal { winner },
            Ok(FebCurrentV1::NativeHalt) => Self::NativeHalt,
            Ok(FebCurrentV1::UnsupportedDecision) => {
                Self::Unsupported("unsupported_native_decision")
            }
            _ => Self::Unsupported("unsupported_numeric_projection"),
        }
    }

    fn wire(&self, episode_id: u64, authority_generation: u64) -> Value {
        let mut state = match self {
            Self::Decision(bound) => json!({
                "state_type": "decision",
                "prompt": bound.prompt,
                "numeric": {
                    "profile": bound.numeric.profile,
                    "card_db_hash": bound.numeric.card_db_hash,
                    "step": bound.numeric.step,
                    "actor": bound.numeric.actor,
                    "observation": wire_tree(&bound.numeric.observation),
                    "actions": bound.numeric.actions.iter().map(wire_tree).collect::<Vec<_>>(),
                },
                "human": bound.human,
            }),
            Self::Terminal { winner } => json!({"state_type": "terminal", "winner": winner}),
            Self::NativeHalt => json!({"state_type": "native_halt"}),
            Self::Unsupported(code) => json!({"state_type": "unsupported", "code": code}),
        };
        state["episode_id"] = json!(episode_id);
        state["authority_generation"] = json!(authority_generation);
        state
    }
}

fn wire_tree(tree: &NumericTree) -> Value {
    json!({"rows": tree.rows(), "topology": tree.topology()})
}

struct Active {
    episode_id: u64,
    core: FebSessionV1,
    projection: Projection,
}

struct Accepted {
    request_id: String,
    receipt: Receipt,
}

#[derive(Default)]
pub struct FebJsonlServerV1 {
    active: Option<Active>,
    authority_generation: u64,
    accepted: Option<Accepted>,
    snapshots: BTreeMap<String, FebSnapshotV1>,
    snapshot_namespace: RandomState,
    next_snapshot: u64,
    last_mutation: Option<(Request, String)>,
}

#[derive(Default)]
struct Applied {
    receipt: Option<Receipt>,
    snapshot_id: Option<String>,
    mutated: bool,
}

impl FebJsonlServerV1 {
    pub fn handle_line(&mut self, line: &str) -> String {
        let request = if line.len() <= MAX_LIMITED_LINE_BYTES_V1 {
            crate::rl::parse_strict_json_value(line)
                .ok()
                .and_then(|value| serde_json::from_value::<Request>(value).ok())
        } else {
            None
        };
        match request {
            Some(request) => self.handle_request(request),
            None => error(None, "invalid_request").to_string(),
        }
    }

    fn handle_request(&mut self, request: Request) -> String {
        let (version, request_id) = request.header();
        if request_id.is_empty() || request_id.len() > 256 {
            return error(None, "invalid_header").to_string();
        }
        if version != SCHEMA_VERSION {
            return error(Some(request_id), "invalid_header").to_string();
        }
        if let Some((previous, reply)) = &self.last_mutation {
            if previous.header().1 == request_id {
                return if previous == &request {
                    reply.clone()
                } else {
                    error(Some(request_id), "conflicting_retry").to_string()
                };
            }
        }
        if let Some(accepted) = &self.accepted {
            if request_id == accepted.request_id.as_str()
                && !matches!(
                    &request,
                    Request::Step { prompt, native_ordinal, submission, .. }
                        if *prompt == accepted.receipt.prompt && *native_ordinal == accepted.receipt.native_ordinal
                            && *submission == accepted.receipt.submission
                )
            {
                return error(Some(request_id), "conflicting_retry").to_string();
            }
        }

        let applied = match self.apply(&request) {
            Ok(applied) => applied,
            Err(code) => return error(Some(request_id), code).to_string(),
        };
        let active = self
            .active
            .as_mut()
            .expect("successful request has a FEB owner");
        let mut reply = header(Some(request_id), "state");
        reply["state"] = active
            .projection
            .wire(active.episode_id, self.authority_generation);
        if let Some(receipt) = applied.receipt {
            reply["receipt"] = json!(receipt);
        }
        if let Some(snapshot_id) = applied.snapshot_id {
            reply["snapshot_id"] = json!(snapshot_id);
        }
        let mut encoded = reply.to_string();
        if encoded.len() >= MAX_LIMITED_LINE_BYTES_V1 {
            // The output bound includes the terminating newline. Keep any
            // already accepted receipt, but withhold an oversized prompt.
            active.projection = Projection::Unsupported("transport_line_limit");
            reply["state"] = active
                .projection
                .wire(active.episode_id, self.authority_generation);
            encoded = reply.to_string();
        }
        if applied.mutated {
            self.last_mutation = Some((request, encoded.clone()));
        }
        encoded
    }

    fn apply(&mut self, request: &Request) -> Result<Applied, &'static str> {
        match request {
            Request::Reset {
                expected_card_db_hash,
                expected_profile,
                episode_id,
                seed,
                starting_player,
                ..
            } => {
                if *expected_card_db_hash != KERNEL_CARDDB_HASH {
                    return Err("catalog_mismatch");
                }
                if expected_profile.as_str() != PROFILE_V1 {
                    return Err("profile_mismatch");
                }
                let generation = self
                    .authority_generation
                    .checked_add(1)
                    .ok_or("authority_exhausted")?;
                let core = FebSessionV1::reset(*seed, *starting_player)
                    .map_err(|_| "unsupported_reset")?;
                let projection = Projection::read(&core, *episode_id, generation);
                self.active = Some(Active {
                    episode_id: *episode_id,
                    core,
                    projection,
                });
                self.authority_generation = generation;
                self.accepted = None;
                self.snapshots.clear();
                Ok(Applied {
                    mutated: true,
                    ..Applied::default()
                })
            }
            Request::Current { .. } => {
                self.active.as_ref().ok_or("no_session")?;
                Ok(Applied::default())
            }
            Request::Step {
                request_id,
                prompt,
                native_ordinal,
                submission,
                ..
            } => self.step(request_id, *prompt, *native_ordinal, *submission),
            Request::Snapshot { .. } => {
                let active = self.active.as_ref().ok_or("no_session")?;
                if self.snapshots.len() >= MAX_SNAPSHOTS {
                    return Err("snapshot_limit");
                }
                let serial = self
                    .next_snapshot
                    .checked_add(1)
                    .ok_or("snapshot_exhausted")?;
                let namespace = self.snapshot_namespace.hash_one(());
                let id = format!("{namespace:016x}{serial:016x}");
                self.snapshots.insert(id.clone(), active.core.snapshot());
                self.next_snapshot = serial;
                Ok(Applied {
                    snapshot_id: Some(id),
                    mutated: true,
                    ..Applied::default()
                })
            }
            Request::Restore { snapshot_id, .. } => {
                let active = self.active.as_mut().ok_or("no_session")?;
                let saved = self.snapshots.get(snapshot_id).ok_or("unknown_snapshot")?;
                let generation = self
                    .authority_generation
                    .checked_add(1)
                    .ok_or("authority_exhausted")?;
                active.core.restore(saved).map_err(|_| "foreign_snapshot")?;
                active.projection = Projection::read(&active.core, active.episode_id, generation);
                self.authority_generation = generation;
                self.accepted = None;
                Ok(Applied {
                    mutated: true,
                    ..Applied::default()
                })
            }
        }
    }

    fn step(
        &mut self,
        request_id: &str,
        prompt: Prompt,
        native_ordinal: u32,
        submission: Submission,
    ) -> Result<Applied, &'static str> {
        let active = self.active.as_mut().ok_or("no_session")?;
        if prompt.episode_id != active.episode_id
            || prompt.authority_generation != self.authority_generation
        {
            return Err("stale_decision");
        }
        if let Some(accepted) = &self.accepted {
            let receipt = accepted.receipt;
            if receipt.prompt.prompt_seq == prompt.prompt_seq {
                if receipt.prompt != prompt {
                    return Err("stale_decision");
                }
                if receipt.native_ordinal != native_ordinal || receipt.submission != submission {
                    return Err("conflicting_retry");
                }
                return Ok(Applied {
                    receipt: Some(receipt),
                    ..Applied::default()
                });
            }
        }
        let Projection::Decision(bound) = &active.projection else {
            return Err("no_executable_prompt");
        };
        if bound.prompt != prompt {
            return Err("stale_decision");
        }
        let ordinal = usize::try_from(native_ordinal).map_err(|_| "invalid_action")?;
        let meaning = bound.human.actions.get(ordinal).ok_or("invalid_action")?;
        match submission {
            Submission::Numeric => active.core.step(&bound.decision, ordinal),
            Submission::Human if matches!(meaning, FebHumanActionV1::Supported { .. }) => {
                active.core.step_human(&bound.decision, ordinal)
            }
            Submission::Human => return Err("unsupported_human_action"),
        }
        .map_err(|_| "execution_refused")?;
        active.projection =
            Projection::read(&active.core, active.episode_id, self.authority_generation);
        let receipt = Receipt {
            prompt,
            native_ordinal,
            submission,
        };
        self.accepted = Some(Accepted {
            request_id: request_id.into(),
            receipt,
        });
        Ok(Applied {
            receipt: Some(receipt),
            mutated: true,
            ..Applied::default()
        })
    }
}

fn header(request_id: Option<&str>, response_type: &str) -> Value {
    json!({
        "protocol": PROTOCOL,
        "schema_version": SCHEMA_VERSION,
        "request_id": request_id,
        "profile": PROFILE_V1,
        "card_db_hash": KERNEL_CARDDB_HASH,
        "response_type": response_type,
    })
}

fn error(request_id: Option<&str>, code: &str) -> Value {
    let mut reply = header(request_id, "error");
    reply["code"] = json!(code);
    reply
}

#[cfg(test)]
pub(super) fn fixture_wire_state(core: &FebSessionV1) -> Value {
    Projection::read(core, 1, 1).wire(1, 1)
}

#[cfg(test)]
#[path = "feb_jsonl_tests.rs"]
mod tests;
