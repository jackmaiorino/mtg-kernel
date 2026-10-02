//! Custom mainboards through a separate JSONL reset/step interface.
//!
//! Card names resolve inside the running binary. Current policy V5 semantics
//! are reused, including its priority suppression and unconditional opening
//! hand. This plumbing does not claim complete Limited rules or FDN support.

use crate::card_def::{card_id_by_name, CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::rl::parse_strict_json_value;
use crate::rl_session::{
    RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionError, RlSessionResponseV1,
    RlSessionTerminalV1,
};
use crate::KERNEL_VERSION;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const LIMITED_PROTOCOL_V1: &str = "kernel_limited_jsonl";
pub const LIMITED_SCHEMA_V1: u32 = 1;
/// A process input bound, not a Magic format rule.
pub const MAX_CUSTOM_DECK_CARDS_V1: usize = 10_000;
pub const MAX_LIMITED_LINE_BYTES_V1: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomCardCountV1 {
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomDeckV1 {
    pub cards: Vec<CustomCardCountV1>,
}

impl CustomDeckV1 {
    pub fn resolve(&self) -> Result<Vec<u16>, String> {
        let mut total = 0usize;
        let mut rows = Vec::new();
        for row in &self.cards {
            if row.count == 0 {
                return Err(format!("zero copies of {:?}", row.name));
            }
            total = total
                .checked_add(row.count as usize)
                .ok_or("deck size overflow")?;
            if total > MAX_CUSTOM_DECK_CARDS_V1 {
                return Err(format!(
                    "deck exceeds the {MAX_CUSTOM_DECK_CARDS_V1}-card input limit"
                ));
            }
            let id =
                card_id_by_name(&row.name).ok_or_else(|| format!("unknown card {:?}", row.name))?;
            let card = &CARD_DEFS[id as usize];
            if card.is_token || !card.has_full_support() {
                return Err(format!(
                    "card {:?} is a token or lacks full engine support",
                    row.name
                ));
            }
            rows.push((id, row.count));
        }
        if total < 40 {
            return Err("Limited mainboard must contain at least 40 cards".to_string());
        }
        let mut ids = Vec::with_capacity(total);
        for (id, count) in rows {
            ids.resize(ids.len() + count as usize, id);
        }
        Ok(ids)
    }
}

/// Row order is significant, matching the imported deck's row/copy order.
/// The registry hash makes identities unambiguous across engine registries.
fn content_identity(ids: &[u16]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"kernel_custom_deck/v1\0");
    hash.update(KERNEL_CARDDB_HASH.to_le_bytes());
    hash.update((ids.len() as u64).to_le_bytes());
    for id in ids {
        hash.update(id.to_le_bytes());
    }
    format!("custom-v1:{:x}", hash.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request_type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LimitedRequestV1 {
    Reset {
        schema_version: u32,
        request_id: String,
        decks: [CustomDeckV1; 2],
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

impl LimitedRequestV1 {
    fn request_id(&self) -> &str {
        match self {
            Self::Reset { request_id, .. } | Self::Step { request_id, .. } => request_id,
        }
    }

    fn schema_version(&self) -> u32 {
        match self {
            Self::Reset { schema_version, .. } | Self::Step { schema_version, .. } => {
                *schema_version
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitedErrorV1 {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "response_type", rename_all = "snake_case")]
pub enum LimitedReplyBodyV1 {
    Decision { decision: Box<RlSessionDecisionV1> },
    Terminal { terminal: RlSessionTerminalV1 },
    Error { error: LimitedErrorV1 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitedReplyV1 {
    pub protocol: String,
    pub schema_version: u32,
    pub request_id: Option<String>,
    pub kernel_version: String,
    pub card_db_hash: u64,
    #[serde(flatten)]
    pub body: LimitedReplyBodyV1,
}

impl LimitedReplyV1 {
    fn new(request_id: Option<String>, body: LimitedReplyBodyV1) -> Self {
        Self {
            protocol: LIMITED_PROTOCOL_V1.to_string(),
            schema_version: LIMITED_SCHEMA_V1,
            request_id,
            kernel_version: KERNEL_VERSION.to_string(),
            card_db_hash: KERNEL_CARDDB_HASH,
            body,
        }
    }

    fn state(request_id: String, response: RlSessionResponseV1) -> Self {
        let body = match response {
            RlSessionResponseV1::Decision(decision) => LimitedReplyBodyV1::Decision {
                decision: Box::new(decision),
            },
            RlSessionResponseV1::Terminal(terminal) => LimitedReplyBodyV1::Terminal { terminal },
        };
        Self::new(Some(request_id), body)
    }

    fn error(request_id: Option<String>, code: &str, message: &str) -> Self {
        Self::new(
            request_id,
            LimitedReplyBodyV1::Error {
                error: LimitedErrorV1 {
                    code: code.to_string(),
                    message: message.to_string(),
                },
            },
        )
    }

    fn session_error(request_id: String, error: RlSessionError) -> Self {
        let code = serde_json::to_value(error.code).expect("session error codes serialize");
        Self::error(
            Some(request_id),
            code.as_str().expect("error code is a string"),
            &error.message,
        )
    }
}

#[derive(Default)]
pub struct LimitedJsonlServerV1 {
    active: Option<RlEpisodeSessionV1>,
    /// The immediate exchange only, matching the existing JSONL retry model.
    last_exchange: Option<(LimitedRequestV1, String)>,
}

impl LimitedJsonlServerV1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_line(&mut self, line: &str) -> String {
        let request = if line.len() <= MAX_LIMITED_LINE_BYTES_V1 {
            parse_strict_json_value(line)
                .and_then(serde_json::from_value::<LimitedRequestV1>)
                .ok()
        } else {
            None
        };
        let Some(request) = request else {
            return serde_json::to_string(&LimitedReplyV1::error(
                None,
                "malformed_request",
                "request does not match the Limited v1 schema",
            ))
            .expect("reply serializes");
        };
        if request.request_id().is_empty() {
            return serde_json::to_string(&LimitedReplyV1::error(
                None,
                "malformed_request",
                "request_id must be nonempty",
            ))
            .expect("reply serializes");
        }
        if request.schema_version() != LIMITED_SCHEMA_V1 {
            return serde_json::to_string(&LimitedReplyV1::error(
                Some(request.request_id().to_string()),
                "schema_version_mismatch",
                "unsupported Limited schema_version",
            ))
            .expect("reply serializes");
        }
        if let Some((previous, response)) = &self.last_exchange {
            if previous.request_id() == request.request_id() {
                if *previous == request {
                    return response.clone();
                }
                return serde_json::to_string(&LimitedReplyV1::error(
                    Some(request.request_id().to_string()),
                    "request_id_reuse_mismatch",
                    "request_id was reused for a different immediate payload",
                ))
                .expect("reply serializes");
            }
        }
        let reply = self.handle_request(&request);
        let response = serde_json::to_string(&reply).expect("reply serializes");
        self.last_exchange = Some((request, response.clone()));
        response
    }

    fn handle_request(&mut self, request: &LimitedRequestV1) -> LimitedReplyV1 {
        let request_id = request.request_id().to_string();
        match request {
            LimitedRequestV1::Reset {
                decks,
                episode_id,
                env_seed,
                max_physical_decisions,
                max_policy_steps,
                ..
            } => {
                let mut ids = [Vec::new(), Vec::new()];
                for (seat, deck) in decks.iter().enumerate() {
                    match deck.resolve() {
                        Ok(resolved) => ids[seat] = resolved,
                        Err(error) => {
                            return LimitedReplyV1::error(
                                Some(request_id),
                                "unsupported_deck",
                                &format!("seat {seat}: {error}"),
                            )
                        }
                    }
                }
                let deck_ids = [&ids[0], &ids[1]].map(|deck| content_identity(deck));
                match RlEpisodeSessionV1::reset_with_custom_decks_v1(
                    *episode_id,
                    *env_seed,
                    *max_physical_decisions,
                    *max_policy_steps,
                    deck_ids,
                    [&ids[0], &ids[1]],
                ) {
                    Ok(session) => {
                        let reply = LimitedReplyV1::state(request_id, session.current_response());
                        self.active = Some(session);
                        reply
                    }
                    Err(error) => LimitedReplyV1::session_error(request_id, error),
                }
            }
            LimitedRequestV1::Step {
                episode_id,
                expected_step,
                selected_index,
                selected_action_id,
                ..
            } => {
                let Some(session) = self.active.as_mut() else {
                    return LimitedReplyV1::error(
                        Some(request_id),
                        "step_before_reset",
                        "reset before stepping",
                    );
                };
                match session.step(
                    *episode_id,
                    *expected_step,
                    *selected_index,
                    selected_action_id,
                ) {
                    Ok(response) => LimitedReplyV1::state(request_id, response),
                    Err(error) => LimitedReplyV1::session_error(request_id, error),
                }
            }
        }
    }
}
