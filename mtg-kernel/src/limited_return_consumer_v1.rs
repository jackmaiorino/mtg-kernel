//! Prospective finite Foundations consumer profile for Alesha, Nine-Lives and
//! Valkyrie's Call. Uses the complete typed V75 owner, Human and numeric paths.
//! This adds no Full capability or playing-strength qualification. It is a
//! trusted two-seat host interface, not a remote-player authorization service.
use crate::{
    card_def::{CARD_DEFS, KERNEL_CARDDB_HASH},
    current_mixed_policy_v1::KaitoSession,
    human_bo3_v1::HumanKaitoSessionV75,
    limited_kaito_session_v16::{
        LimitedKaitoJsonlServerV16, LimitedKaitoReplyV16, LimitedKaitoRequestV16,
    },
    rl::PlayerSeatV1,
    rl_session::{FastActorSessionV1, KaitoSessionV75, SessionDeckIdsV1},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFILE_V1: &str = "fdn-return-consumer/v1";
pub const PARTIAL_PRINTINGS_V1: [u16; 3] = [457, 458, 459];

pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [PROFILE_V1, include_str!("limited_return_consumer_v1.rs"),
            include_str!("rl_session/limited_return_v1.rs"), include_str!("rl_session/kaito_v75.rs"),
            include_str!("rl.rs"), include_str!("bin/kernel_limited_return_env_v1.rs")] {
            hash.update([0]); hash.update(source.as_bytes());
        }
        hash.update(crate::limited_kaito_session_v16::producer_sha256_v16());
        hash.update(crate::current_mixed_policy_v1::feature_identity());
        format!("{:x}", hash.finalize())
    }).clone()
}

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

pub(crate) fn preflight(config: &ResetV1) -> Result<(), String> {
    if !cfg!(all(feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures"))) {
        return Err("return profile requires the Foundations catalog".into());
    }
    if config.expected_card_db_hash != KERNEL_CARDDB_HASH {
        return Err("catalog mismatch".into());
    }
    if config.max_physical_decisions == 0 || config.max_policy_steps == 0
        || config.mainboards.iter().any(|deck| !(40..=crate::limited_session_v1::MAX_CUSTOM_DECK_CARDS_V1).contains(&deck.len()))
    {
        return Err("invalid return profile reset".into());
    }
    // These are exhaustive comparators over actual CardDefs, programs and
    // triggers. Numeric expansion later validates the same typed meanings.
    use crate::rules_meaning_graveyard_return_v61::{self as graveyard, FamilyV61};
    let exact = [
        graveyard::printed_meaning(FamilyV61::Alesha)?.card_id,
        graveyard::printed_meaning(FamilyV61::NineLives)?.card_id,
        crate::rules_meaning_valkyries_call_v62::printed_meaning()?.card_id,
    ];
    if exact != PARTIAL_PRINTINGS_V1 {
        return Err("return profile printing identity changed".into());
    }
    for deck in &config.mainboards {
        for &id in deck {
            let def = CARD_DEFS.get(usize::from(id)).ok_or("unknown declared card")?;
            if def.is_token || !(def.has_full_support() || (exact.contains(&id) && def.is_executable())) {
                return Err(format!("card {id} is outside the return consumer profile"));
            }
        }
        crate::rl::require_declared_cards_for_scope(crate::rl::CommonProjectionScopeV42::KaitoV75, deck)?;
    }
    Ok(())
}

fn native(config: &ResetV1) -> Result<FastActorSessionV1, String> {
    FastActorSessionV1::reset_limited_return_v1(config).map_err(|error| format!("{error:?}"))
}

/// Genuine native owner is selected before its first surfaced decision.
/// Captures, score/apply, sampling and restore use the existing sealed V75 API.
pub fn reset_numeric_v1(config: &ResetV1) -> Result<KaitoSession, String> {
    KaitoSession::from_session(native(config)?).map_err(|error| error.to_string())
}

/// A fixed-seat Human owner over the same exact profile and native decisions.
pub fn reset_human_v1(config: &ResetV1, human_seat: PlayerSeatV1) -> Result<HumanKaitoSessionV75, String> {
    HumanKaitoSessionV75::from_session(native(config)?, human_seat).map_err(|error| error.to_string())
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
    Reset { config: ResetV1 },
    Current,
    Step {
        episode_id: u64,
        authority_generation: u64,
        actor: PlayerSeatV1,
        prompt_seq: u64,
        action_index: u32,
    },
    Snapshot,
    Restore { snapshot_id: u64 },
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
    State { payload: Box<LimitedKaitoReplyV16> },
    Error { code: String },
}

#[derive(Default)]
pub struct JsonlServerV1 {
    inner: LimitedKaitoJsonlServerV16,
    last_exchange: Option<(RequestV1, ReplyV1)>,
}

impl JsonlServerV1 {
    pub fn new() -> Self { Self::default() }

    fn error(id: Option<String>, code: &str) -> ReplyV1 {
        ReplyV1 { profile: PROFILE_V1.into(), producer_sha256: producer_sha256_v1(), request_id: id, body: ReplyBodyV1::Error { code: code.into() } }
    }

    pub fn handle_line(&mut self, line: &str) -> String {
        let parsed = (line.len() <= crate::limited_session_v1::MAX_LIMITED_LINE_BYTES_V1)
            .then(|| crate::rl::parse_strict_json_value(line).ok())
            .flatten()
            .and_then(|value| serde_json::from_value::<RequestV1>(value).ok());
        let reply = match parsed {
            Some(request) => self.handle_request(request),
            None => Self::error(None, "invalid_request"),
        };
        serde_json::to_string(&reply).expect("safe return consumer reply serializes")
    }

    pub fn handle_request(&mut self, request: RequestV1) -> ReplyV1 {
        if request.profile != PROFILE_V1 || request.request_id.is_empty() || request.request_id.len() > 256 {
            return Self::error(Some(request.request_id), "invalid_header");
        }
        if let Some((previous, reply)) = &self.last_exchange {
            if previous.request_id == request.request_id {
                return if previous == &request { reply.clone() }
                    else { Self::error(Some(request.request_id), "conflicting_retry") };
            }
        }
        let id = request.request_id.clone();
        let result = self.apply(&request.command, id.clone());
        let reply = match result {
            Ok(payload) => ReplyV1 { profile: PROFILE_V1.into(), producer_sha256: producer_sha256_v1(), request_id: Some(id), body: ReplyBodyV1::State { payload: Box::new(payload) } },
            Err(code) => Self::error(Some(id), code),
        };
        self.last_exchange = Some((request, reply.clone()));
        reply
    }

    fn apply(&mut self, command: &CommandV1, request_id: String) -> Result<LimitedKaitoReplyV16, &'static str> {
        let schema_version = crate::limited_kaito_session_v16::SCHEMA_VERSION;
        let delegated = match command {
            CommandV1::Reset { config } => {
                let core = KaitoSessionV75::from_session(native(config).map_err(|_| "unsupported_reset")?)
                    .map_err(|_| "unsupported_projection")?;
                // Failed construction leaves the previous owner untouched.
                self.inner.install_profile_core(core)?;
                LimitedKaitoRequestV16::Current { schema_version, request_id }
            }
            CommandV1::Current => LimitedKaitoRequestV16::Current { schema_version, request_id },
            CommandV1::Step { episode_id, authority_generation, actor, prompt_seq, action_index } =>
                LimitedKaitoRequestV16::Step { schema_version, request_id, episode_id: *episode_id,
                    authority_generation: *authority_generation, actor: *actor, prompt_seq: *prompt_seq, action_index: *action_index },
            CommandV1::Snapshot => LimitedKaitoRequestV16::Snapshot { schema_version, request_id },
            CommandV1::Restore { snapshot_id } => LimitedKaitoRequestV16::Restore { schema_version, request_id, snapshot_id: *snapshot_id },
        };
        Ok(self.inner.handle_request(delegated))
    }
}

#[cfg(all(test, feature = "limited-fdn-fixtures", not(feature = "standard-magezero-fixtures")))]
mod tests;
