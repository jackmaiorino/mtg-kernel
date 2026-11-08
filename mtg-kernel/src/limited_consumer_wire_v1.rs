//! Shared actor-facing envelope for explicitly preflighted consumer profiles.
use crate::{
    limited_kaito_session_v16::{
        LimitedKaitoJsonlServerV16, LimitedKaitoReplyV16, LimitedKaitoRequestV16,
    },
    rl::PlayerSeatV1,
    rl_session::{FastActorSessionV1, KaitoSessionV75, SessionDeckIdsV1},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetV1 {
    pub expected_card_db_hash: u64,
    pub episode_id: u64,
    pub env_seed: u64,
    pub starting_player: PlayerSeatV1,
    pub deck_ids: SessionDeckIdsV1,
    pub mainboards: [Vec<u16>; 2],
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestV1 {
    pub profile: String,
    pub request_id: String,
    pub command: CommandV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommandV1 {
    Reset {
        config: ResetV1,
    },
    Current,
    Step {
        episode_id: u64,
        authority_generation: u64,
        actor: PlayerSeatV1,
        prompt_seq: u64,
        action_index: u32,
    },
    Snapshot,
    Restore {
        snapshot_id: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReplyV1 {
    pub profile: String,
    pub producer_sha256: String,
    pub request_id: Option<String>,
    #[serde(flatten)]
    pub body: ReplyBodyV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReplyBodyV1 {
    /// V16 is the payload vocabulary only. Its Full-only Reset is not exposed
    /// inside this profile's request grammar or advertised as this admission.
    State {
        payload: Box<LimitedKaitoReplyV16>,
    },
    Error {
        code: String,
    },
}

pub(crate) struct ProfileWireV1 {
    inner: LimitedKaitoJsonlServerV16,
    profile: &'static str,
    producer: fn() -> String,
    reset: fn(&ResetV1) -> Result<FastActorSessionV1, String>,
    last_exchange: Option<(RequestV1, ReplyV1)>,
}

impl ProfileWireV1 {
    pub(crate) fn new(
        profile: &'static str,
        producer: fn() -> String,
        reset: fn(&ResetV1) -> Result<FastActorSessionV1, String>,
    ) -> Self {
        Self {
            inner: LimitedKaitoJsonlServerV16::new(),
            profile,
            producer,
            reset,
            last_exchange: None,
        }
    }

    #[cfg(all(
        test,
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn install_fixture_core(&mut self, core: KaitoSessionV75) {
        self.inner.install_profile_core(core).unwrap();
    }

    fn error(&self, id: Option<String>, code: &str) -> ReplyV1 {
        ReplyV1 {
            profile: self.profile.into(),
            producer_sha256: (self.producer)(),
            request_id: id,
            body: ReplyBodyV1::Error { code: code.into() },
        }
    }

    pub fn handle_line(&mut self, line: &str) -> String {
        let parsed = (line.len() <= crate::limited_session_v1::MAX_LIMITED_LINE_BYTES_V1)
            .then(|| crate::rl::parse_strict_json_value(line).ok())
            .flatten()
            .and_then(|value| serde_json::from_value::<RequestV1>(value).ok());
        let reply = match parsed {
            Some(request) => self.handle_request(request),
            None => self.error(None, "invalid_request"),
        };
        serde_json::to_string(&reply).expect("safe consumer reply serializes")
    }

    pub fn handle_request(&mut self, request: RequestV1) -> ReplyV1 {
        if request.profile != self.profile
            || request.request_id.is_empty()
            || request.request_id.len() > 256
        {
            return self.error(Some(request.request_id), "invalid_header");
        }
        if let Some((previous, reply)) = &self.last_exchange {
            if previous.request_id == request.request_id {
                return if previous == &request {
                    reply.clone()
                } else {
                    self.error(Some(request.request_id), "conflicting_retry")
                };
            }
        }
        let id = request.request_id.clone();
        let result = self.apply(&request.command, id.clone());
        let reply = match result {
            Ok(payload) => ReplyV1 {
                profile: self.profile.into(),
                producer_sha256: (self.producer)(),
                request_id: Some(id),
                body: ReplyBodyV1::State {
                    payload: Box::new(payload),
                },
            },
            Err(code) => self.error(Some(id), code),
        };
        self.last_exchange = Some((request, reply.clone()));
        reply
    }

    fn apply(
        &mut self,
        command: &CommandV1,
        request_id: String,
    ) -> Result<LimitedKaitoReplyV16, &'static str> {
        let schema_version = crate::limited_kaito_session_v16::SCHEMA_VERSION;
        let delegated = match command {
            CommandV1::Reset { config } => {
                let core = KaitoSessionV75::from_session(
                    (self.reset)(config).map_err(|_| "unsupported_reset")?,
                )
                .map_err(|_| "unsupported_projection")?;
                // Failed construction leaves the previous owner untouched.
                self.inner.install_profile_core(core)?;
                LimitedKaitoRequestV16::Current {
                    schema_version,
                    request_id,
                }
            }
            CommandV1::Current => LimitedKaitoRequestV16::Current {
                schema_version,
                request_id,
            },
            CommandV1::Step {
                episode_id,
                authority_generation,
                actor,
                prompt_seq,
                action_index,
            } => LimitedKaitoRequestV16::Step {
                schema_version,
                request_id,
                episode_id: *episode_id,
                authority_generation: *authority_generation,
                actor: *actor,
                prompt_seq: *prompt_seq,
                action_index: *action_index,
            },
            CommandV1::Snapshot => LimitedKaitoRequestV16::Snapshot {
                schema_version,
                request_id,
            },
            CommandV1::Restore { snapshot_id } => LimitedKaitoRequestV16::Restore {
                schema_version,
                request_id,
                snapshot_id: *snapshot_id,
            },
        };
        Ok(self.inner.handle_request(delegated))
    }
}
