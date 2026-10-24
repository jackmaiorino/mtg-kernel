//! Custom mainboards through a separate JSONL reset/step interface.
//!
//! Card names resolve inside the running binary. Schema 1 retains H2 priority
//! suppression. Schema 2 explicitly opts into engine priority windows.
//! Schema 3 adds Foundations damage assignment and trample. These modes
//! retain the unconditional opening hand and policy V5 action representation.
//! Schema 4 adds London mulligans before normal turn progression.
//! This plumbing does not claim complete Limited rules or FDN support.

use crate::card_def::{card_id_by_name, CARD_DEFS, KERNEL_CARDDB_HASH};
use crate::rl::parse_strict_json_value;
use crate::rl_session::{
    RlEpisodeSessionV1, RlSessionDecisionV1, RlSessionError, RlSessionResponseV1,
    RlSessionTerminalV1,
};
use crate::surface_v2::PriorityModeV1;
use crate::KERNEL_VERSION;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const LIMITED_PROTOCOL_V1: &str = "kernel_limited_jsonl";
pub const LIMITED_SCHEMA_V1: u32 = 1;
pub const LIMITED_ENGINE_PRIORITY_SCHEMA_V1: u32 = 2;
pub const LIMITED_FOUNDATIONS_COMBAT_SCHEMA_V1: u32 = 3;
pub const LIMITED_LONDON_MULLIGANS_SCHEMA_V1: u32 = 4;
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
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::ExistingV40)
    }
    pub(crate) fn resolve_next_five_v59(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::NextFiveV59)
    }
    pub(crate) fn resolve_graveyard_return_v61(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::GraveyardReturnV61)
    }
    pub(crate) fn resolve_call_v62(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::CallV62)
    }
    pub(crate) fn resolve_end_turn_v68(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::EndTurnV68)
    }
    pub(crate) fn resolve_genesis_wave_v69(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::GenesisWaveV69)
    }
    pub(crate) fn resolve_vivien_v70(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::VivienV70)
    }
    pub(crate) fn resolve_muldrotha_v71(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::MuldrothaV71)
    }
    pub(crate) fn resolve_tinybones_v72(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::TinybonesV72)
    }
    pub(crate) fn resolve_liliana_v73(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::LilianaV73)
    }
    pub(crate) fn resolve_chandra_v74(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::ChandraV74)
    }

    pub(crate) fn resolve_kaito_v75(&self) -> Result<Vec<u16>, String> {
        self.resolve_for_scope(crate::rl::CommonProjectionScopeV42::KaitoV75)
    }

    fn resolve_for_scope(
        &self,
        scope: crate::rl::CommonProjectionScopeV42,
    ) -> Result<Vec<u16>, String> {
        let cards = self.resolve_full()?;
        crate::rl::require_declared_cards_for_scope(scope, &cards)?;
        Ok(cards)
    }
    fn resolve_full(&self) -> Result<Vec<u16>, String> {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_rules: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mulligan_rules: Option<String>,
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
            priority_mode: None,
            combat_rules: None,
            mulligan_rules: None,
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
    priority_mode: PriorityModeV1,
    foundations_combat: bool,
    london_mulligans: bool,
    active: Option<RlEpisodeSessionV1>,
    /// The immediate exchange only, matching the existing JSONL retry model.
    last_exchange: Option<(LimitedRequestV1, String)>,
}

impl LimitedJsonlServerV1 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Immutable process mode; schema-1 requests are refused in this mode.
    pub fn new_with_engine_priority_v1() -> Self {
        Self {
            priority_mode: PriorityModeV1::EngineWindowsV1,
            ..Self::default()
        }
    }

    /// Foundations assignment and trample, with full engine priority windows.
    pub fn new_with_foundations_combat_v1() -> Self {
        Self {
            priority_mode: PriorityModeV1::EngineWindowsV1,
            foundations_combat: true,
            ..Self::default()
        }
    }

    /// Full engine priority, Foundations combat and two-player London mulligans.
    pub fn new_with_london_mulligans_v1() -> Self {
        Self {
            priority_mode: PriorityModeV1::EngineWindowsV1,
            foundations_combat: true,
            london_mulligans: true,
            ..Self::default()
        }
    }

    fn schema_version(&self) -> u32 {
        if self.london_mulligans {
            return LIMITED_LONDON_MULLIGANS_SCHEMA_V1;
        }
        if self.foundations_combat {
            return LIMITED_FOUNDATIONS_COMBAT_SCHEMA_V1;
        }
        match self.priority_mode {
            PriorityModeV1::HarnessV2 => LIMITED_SCHEMA_V1,
            PriorityModeV1::EngineWindowsV1 => LIMITED_ENGINE_PRIORITY_SCHEMA_V1,
        }
    }

    fn serialize_reply(&self, mut reply: LimitedReplyV1) -> String {
        reply.schema_version = self.schema_version();
        if self.london_mulligans {
            reply.mulligan_rules = Some("london_v1".to_string());
        }
        if self.foundations_combat {
            reply.combat_rules = Some("foundations_v1".to_string());
        }
        if self.priority_mode == PriorityModeV1::EngineWindowsV1 {
            reply.priority_mode = Some("engine_windows_v1".to_string());
        }
        serde_json::to_string(&reply).expect("reply serializes")
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
            return self.serialize_reply(LimitedReplyV1::error(
                None,
                "malformed_request",
                if self.priority_mode == PriorityModeV1::HarnessV2 {
                    "request does not match the Limited v1 schema"
                } else {
                    "request does not match the Limited schema 2 request"
                },
            ));
        };
        if request.request_id().is_empty() {
            return self.serialize_reply(LimitedReplyV1::error(
                None,
                "malformed_request",
                "request_id must be nonempty",
            ));
        }
        if request.schema_version() != self.schema_version() {
            return self.serialize_reply(LimitedReplyV1::error(
                Some(request.request_id().to_string()),
                "schema_version_mismatch",
                "unsupported Limited schema_version",
            ));
        }
        if let Some((previous, response)) = &self.last_exchange {
            if previous.request_id() == request.request_id() {
                if *previous == request {
                    return response.clone();
                }
                return self.serialize_reply(LimitedReplyV1::error(
                    Some(request.request_id().to_string()),
                    "request_id_reuse_mismatch",
                    "request_id was reused for a different immediate payload",
                ));
            }
        }
        let reply = self.handle_request(&request);
        let response = self.serialize_reply(reply);
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
                let session = if self.london_mulligans {
                    RlEpisodeSessionV1::reset_with_custom_decks_and_london_v1(
                        *episode_id,
                        *env_seed,
                        *max_physical_decisions,
                        *max_policy_steps,
                        deck_ids,
                        [&ids[0], &ids[1]],
                        self.priority_mode,
                        self.foundations_combat,
                        true,
                    )
                } else {
                    RlEpisodeSessionV1::reset_with_custom_decks_v1(
                        *episode_id,
                        *env_seed,
                        *max_physical_decisions,
                        *max_policy_steps,
                        deck_ids,
                        [&ids[0], &ids[1]],
                        self.priority_mode,
                        self.foundations_combat,
                    )
                };
                match session {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn declared_scope_classification_never_overrides_original_full_preflight() {
        use crate::rl::CommonProjectionScopeV42 as Scope;
        for (name, next_five) in [
            ("Midnight Snack", true),
            ("Extravagant Replication", true),
            ("Infernal Vessel", true),
            ("Giada, Font of Hope", true),
            ("Goblin Negotiation", true),
            ("Alesha, Who Laughs at Fate", false),
            ("Nine-Lives Familiar", false),
        ] {
            let id = card_id_by_name(name).unwrap();
            assert_eq!(CARD_DEFS[usize::from(id)].has_full_support(), next_five);
            assert!(
                crate::rl::require_declared_cards_for_scope(Scope::ExistingV40, &[id]).is_err()
            );
            assert_eq!(
                crate::rl::require_declared_cards_for_scope(Scope::NextFiveV59, &[id]).is_ok(),
                next_five
            );
            assert!(
                crate::rl::require_declared_cards_for_scope(Scope::GraveyardReturnV61, &[id])
                    .is_ok()
            );
            let deck = CustomDeckV1 {
                cards: vec![CustomCardCountV1 {
                    name: name.into(),
                    count: 40,
                }],
            };
            if next_five {
                let resolved = deck.resolve_full().unwrap();
                assert_eq!(resolved, vec![id; 40]);
                assert!(
                    deck.resolve().is_err(),
                    "old consumer still refuses complete new rules"
                );
                assert_eq!(deck.resolve_next_five_v59().unwrap(), resolved);
                assert_eq!(deck.resolve_graveyard_return_v61().unwrap(), resolved);
            } else {
                let original = deck.resolve_full().unwrap_err();
                assert!(original.contains("lacks full engine support"));
                assert_eq!(deck.resolve().unwrap_err(), original);
                assert_eq!(deck.resolve_next_five_v59().unwrap_err(), original);
                assert_eq!(deck.resolve_graveyard_return_v61().unwrap_err(), original);
            }
        }
        let basics = CustomDeckV1 {
            cards: vec![CustomCardCountV1 {
                name: "Forest".into(),
                count: 40,
            }],
        };
        assert_eq!(
            basics.resolve().unwrap(),
            basics.resolve_next_five_v59().unwrap()
        );
        assert_eq!(
            basics.resolve().unwrap(),
            basics.resolve_graveyard_return_v61().unwrap()
        );
        let zero = CustomDeckV1 {
            cards: vec![CustomCardCountV1 {
                name: "Alesha, Who Laughs at Fate".into(),
                count: 0,
            }],
        };
        assert_eq!(
            zero.resolve().unwrap_err(),
            zero.resolve_graveyard_return_v61().unwrap_err()
        );
        assert!(zero.resolve().unwrap_err().starts_with("zero copies"));
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn complete_wire_failed_partial_resets_retain_current_prompt_both_seats() {
        use serde_json::{json, Value};
        fn check(
            mut handle: impl FnMut(&str) -> String,
            schema: u32,
            actor: crate::rl::PlayerSeatV1,
        ) {
            let reset = json!({"request_type":"reset", "schema_version":schema, "request_id":"initial",
                "expected_card_db_hash":KERNEL_CARDDB_HASH, "episode_id":7, "env_seed":123,
                "starting_player":actor, "max_physical_decisions":4096, "max_policy_steps":4096,
                "decks":[{"cards":[{"name":"Forest","count":40}]},{"cards":[{"name":"Island","count":40}]}]});
            let initial: Value = serde_json::from_str(&handle(&reset.to_string())).unwrap();
            assert_ne!(initial["response_type"], "error");
            let current = json!({"request_type":"current", "schema_version":schema,"request_id":"same-current"});
            let before = handle(&current.to_string());
            for name in [
                "Alesha, Who Laughs at Fate",
                "Nine-Lives Familiar",
                "Valkyrie's Call",
            ] {
                let mut bad = reset.clone();
                bad["request_id"] = json!(name);
                bad["episode_id"] = json!(999);
                bad["decks"][0]["cards"] =
                    json!([{"name":"Forest","count":39},{"name":name,"count":1}]);
                let rejected: Value = serde_json::from_str(&handle(&bad.to_string())).unwrap();
                assert_eq!(rejected["code"], "unsupported_deck");
                assert_eq!(handle(&current.to_string()), before);
            }
        }
        for actor in [crate::rl::PlayerSeatV1::P0, crate::rl::PlayerSeatV1::P1] {
            let mut v1 = crate::limited_common_session_v1::LimitedCommonJsonlServerV1::new();
            check(|line| v1.handle_line(line), 1, actor);
            let mut v2 = crate::limited_common_session_v2::LimitedCommonJsonlServerV2::new();
            check(|line| v2.handle_line(line), 2, actor);
            let mut v3 = crate::limited_common_session_v3::LimitedCommonJsonlServerV3::new();
            check(|line| v3.handle_line(line), 3, actor);
            let mut v4 = crate::limited_elenda_session_v4::LimitedElendaJsonlServerV4::new();
            check(|line| v4.handle_line(line), 4, actor);
            let mut v5 = crate::limited_prepared_session_v5::LimitedPreparedJsonlServerV5::new();
            check(|line| v5.handle_line(line), 5, actor);
            let mut v6 = crate::limited_next_five_session_v6::LimitedNextFiveJsonlServerV6::new();
            check(|line| v6.handle_line(line), 6, actor);
            let mut v7 = crate::limited_graveyard_return_session_v7::LimitedGraveyardReturnJsonlServerV7::new();
            check(|line| v7.handle_line(line), 7, actor);
        }
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn legacy_wire_failed_partial_resets_keep_active_world_and_prompt_for_each_mode() {
        use serde_json::json;
        for mut server in [
            LimitedJsonlServerV1::new(),
            LimitedJsonlServerV1::new_with_engine_priority_v1(),
            LimitedJsonlServerV1::new_with_foundations_combat_v1(),
            LimitedJsonlServerV1::new_with_london_mulligans_v1(),
        ] {
            let reset = json!({"request_type":"reset", "schema_version":server.schema_version(),
                "request_id":"initial", "episode_id":7, "env_seed":123,
                "max_physical_decisions":4096, "max_policy_steps":4096,
                "decks":[{"cards":[{"name":"Forest","count":40}]},{"cards":[{"name":"Island","count":40}]}]});
            let initial: LimitedReplyV1 =
                serde_json::from_str(&server.handle_line(&reset.to_string())).unwrap();
            assert!(!matches!(initial.body, LimitedReplyBodyV1::Error { .. }));
            let active = server.active.as_ref().unwrap();
            let before = (
                active.privileged_environment_hash(),
                active.current_response(),
            );
            for name in [
                "Midnight Snack",
                "Alesha, Who Laughs at Fate",
                "Nine-Lives Familiar",
            ] {
                let mut bad = reset.clone();
                bad["request_id"] = json!(name);
                bad["episode_id"] = json!(999);
                bad["decks"][0]["cards"] =
                    json!([{"name":"Forest","count":39},{"name":name,"count":1}]);
                let rejected: LimitedReplyV1 =
                    serde_json::from_str(&server.handle_line(&bad.to_string())).unwrap();
                let LimitedReplyBodyV1::Error { error } = rejected.body else {
                    panic!("Partial was admitted")
                };
                assert_eq!(error.code, "unsupported_deck");
                let active = server.active.as_ref().unwrap();
                assert_eq!(
                    (
                        active.privileged_environment_hash(),
                        active.current_response()
                    ),
                    before
                );
            }
        }
    }

    #[test]
    fn london_session_snapshot_restores_pending_bottom_menu_binding_and_transition() {
        let cards = [card_id_by_name("Forest").unwrap(); 40];
        let mut session = RlEpisodeSessionV1::reset_with_custom_decks_and_london_v1(
            7,
            123,
            8192,
            16384,
            [content_identity(&cards), content_identity(&cards)],
            [&cards, &cards],
            PriorityModeV1::EngineWindowsV1,
            true,
            true,
        )
        .unwrap();
        for action_index in [1, 0] {
            let RlSessionResponseV1::Decision(decision) = session.current_response() else {
                panic!("opening decision")
            };
            let action = &decision.legal_actions[action_index];
            session
                .step(
                    decision.episode_id,
                    decision.step,
                    action.selected_index,
                    &action.stable_id,
                )
                .unwrap();
        }
        let before = session.current_response();
        let RlSessionResponseV1::Decision(decision) = &before else {
            panic!("bottom decision")
        };
        assert_eq!(decision.legal_actions.len(), 7);
        assert!(matches!(
            decision.legal_actions[0].semantic,
            crate::rl::ActionSemanticV1::ChooseLondonBottom { remaining: 1, .. }
        ));
        let snapshot = session.snapshot_v5();
        let before_hash = session.privileged_environment_hash();
        let action = &decision.legal_actions[0];
        assert!(session
            .step(
                decision.episode_id,
                decision.step,
                action.selected_index,
                "stale"
            )
            .is_err());
        assert_eq!(session.current_response(), before);
        assert_eq!(session.privileged_environment_hash(), before_hash);
        let after = session
            .step(
                decision.episode_id,
                decision.step,
                action.selected_index,
                &action.stable_id,
            )
            .unwrap();
        let after_hash = session.privileged_environment_hash();
        session.restore_v5(&snapshot);
        assert_eq!(session.current_response(), before);
        assert_eq!(session.privileged_environment_hash(), before_hash);
        assert_eq!(
            session
                .step(
                    decision.episode_id,
                    decision.step,
                    action.selected_index,
                    &action.stable_id
                )
                .unwrap(),
            after
        );
        assert_eq!(session.privileged_environment_hash(), after_hash);
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn foundations_session_snapshot_restores_second_power_spell_target_and_binding() {
        use crate::rl::ActionSemanticV1;
        let forest = card_id_by_name("Forest").unwrap();
        let elves = card_id_by_name("Llanowar Elves").unwrap();
        for name in ["Bite Down", "Felling Blow"] {
            let spell = card_id_by_name(name).unwrap();
            let cards = [vec![forest; 20], vec![elves; 12], vec![spell; 8]].concat();
            let identity = content_identity(&cards);
            let mut session = RlEpisodeSessionV1::reset_with_custom_decks_v1(
                7,
                123,
                8192,
                16384,
                [identity.clone(), identity],
                [&cards, &cards],
                PriorityModeV1::EngineWindowsV1,
                true,
            )
            .unwrap();
            let mut verified = false;
            for _ in 0..4096 {
                let before = session.current_response();
                let RlSessionResponseV1::Decision(decision) = &before else {
                    panic!("{name} game ended before the second target choice");
                };
                if matches!(&decision.legal_actions[0].semantic,
                    ActionSemanticV1::ChooseTarget { source, remaining: 1, .. } if source.card_db_id == spell)
                {
                    let snapshot = session.snapshot_v5();
                    let before_hash = session.privileged_environment_hash();
                    let action = &decision.legal_actions[0];
                    let after = session
                        .step(
                            decision.episode_id,
                            decision.step,
                            action.selected_index,
                            &action.stable_id,
                        )
                        .unwrap();
                    let after_hash = session.privileged_environment_hash();
                    session.restore_v5(&snapshot);
                    assert_eq!(session.current_response(), before);
                    assert_eq!(session.privileged_environment_hash(), before_hash);
                    assert_eq!(
                        session
                            .step(
                                decision.episode_id,
                                decision.step,
                                action.selected_index,
                                &action.stable_id
                            )
                            .unwrap(),
                        after
                    );
                    assert_eq!(session.privileged_environment_hash(), after_hash);
                    verified = true;
                    break;
                }
                let rank = |semantic: &ActionSemanticV1| match semantic {
                    ActionSemanticV1::PlayLand { .. } => 0,
                    ActionSemanticV1::CastSpell { .. } => 1,
                    ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                    | ActionSemanticV1::ChooseBlockerInclusion { include: true, .. } => 2,
                    ActionSemanticV1::Pass { .. } => 10,
                    ActionSemanticV1::ActivateManaAbility { .. } => 11,
                    _ => 5,
                };
                let action = decision
                    .legal_actions
                    .iter()
                    .min_by_key(|action| rank(&action.semantic))
                    .unwrap();
                session
                    .step(
                        decision.episode_id,
                        decision.step,
                        action.selected_index,
                        &action.stable_id,
                    )
                    .unwrap();
            }
            assert!(verified, "{name} second target choice was not reached");
        }
    }
    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn foundations_session_snapshot_restores_lookout_discard_choice_and_binding() {
        use crate::rl::ActionSemanticV1;
        let island = card_id_by_name("Island").unwrap();
        let lookout = card_id_by_name("Strix Lookout").unwrap();
        let cards = [vec![island; 30], vec![lookout; 10]].concat();
        let identity = content_identity(&cards);
        let mut session = RlEpisodeSessionV1::reset_with_custom_decks_v1(
            7,
            123,
            8192,
            16384,
            [identity.clone(), identity],
            [&cards, &cards],
            PriorityModeV1::EngineWindowsV1,
            true,
        )
        .unwrap();
        for _ in 0..4096 {
            let before = session.current_response();
            let RlSessionResponseV1::Decision(decision) = &before else {
                panic!("Lookout game ended before a discard choice");
            };
            if matches!(
                decision.legal_actions[0].semantic,
                ActionSemanticV1::Discard { .. }
            ) {
                let snapshot = session.snapshot_v5();
                let before_hash = session.privileged_environment_hash();
                let action = &decision.legal_actions[0];
                let after = session
                    .step(
                        decision.episode_id,
                        decision.step,
                        action.selected_index,
                        &action.stable_id,
                    )
                    .unwrap();
                let after_hash = session.privileged_environment_hash();
                session.restore_v5(&snapshot);
                assert_eq!(session.current_response(), before);
                assert_eq!(session.privileged_environment_hash(), before_hash);
                assert_eq!(
                    session
                        .step(
                            decision.episode_id,
                            decision.step,
                            action.selected_index,
                            &action.stable_id
                        )
                        .unwrap(),
                    after
                );
                assert_eq!(session.privileged_environment_hash(), after_hash);
                return;
            }
            let action = decision
                .legal_actions
                .iter()
                .min_by_key(|action| match &action.semantic {
                    ActionSemanticV1::PlayLand { .. } => 0,
                    ActionSemanticV1::CastSpell { .. } => 1,
                    ActionSemanticV1::ActivateAbility { source, .. }
                        if source.card_db_id == lookout =>
                    {
                        2
                    }
                    ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
                    | ActionSemanticV1::ChooseBlockerInclusion { include: false, .. } => 3,
                    ActionSemanticV1::Pass { .. } => 10,
                    ActionSemanticV1::ActivateManaAbility { .. } => 11,
                    _ => 5,
                })
                .unwrap();
            session
                .step(
                    decision.episode_id,
                    decision.step,
                    action.selected_index,
                    &action.stable_id,
                )
                .unwrap();
        }
        panic!("Lookout discard choice was not reached");
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn foundations_session_snapshot_restores_kicker_choice_and_binding() {
        use crate::rl::ActionSemanticV1;
        let forest = card_id_by_name("Forest").unwrap();
        for name in ["Gnarlid Colony"] {
            let spell = card_id_by_name(name).unwrap();
            let cards = [vec![forest; 30], vec![spell; 10]].concat();
            let identity = content_identity(&cards);
            let mut session = RlEpisodeSessionV1::reset_with_custom_decks_v1(
                7,
                123,
                8192,
                16384,
                [identity.clone(), identity],
                [&cards, &cards],
                PriorityModeV1::EngineWindowsV1,
                true,
            )
            .unwrap();
            let mut verified = false;
            for _ in 0..4096 {
                let before = session.current_response();
                let RlSessionResponseV1::Decision(decision) = &before else {
                    panic!("{name} game ended before the kicker choice");
                };
                if matches!(&decision.legal_actions[0].semantic,
                    ActionSemanticV1::ChooseKicker { source, .. } if source.card_db_id == spell)
                {
                    let snapshot = session.snapshot_v5();
                    let before_hash = session.privileged_environment_hash();
                    let action = &decision.legal_actions[1];
                    let after = session
                        .step(
                            decision.episode_id,
                            decision.step,
                            action.selected_index,
                            &action.stable_id,
                        )
                        .unwrap();
                    let after_hash = session.privileged_environment_hash();
                    session.restore_v5(&snapshot);
                    assert_eq!(session.current_response(), before);
                    assert_eq!(session.privileged_environment_hash(), before_hash);
                    assert_eq!(
                        session
                            .step(
                                decision.episode_id,
                                decision.step,
                                action.selected_index,
                                &action.stable_id
                            )
                            .unwrap(),
                        after
                    );
                    assert_eq!(session.privileged_environment_hash(), after_hash);
                    verified = true;
                    break;
                }
                let rank = |semantic: &ActionSemanticV1| match semantic {
                    ActionSemanticV1::PlayLand { .. } => 0,
                    ActionSemanticV1::CastSpell { .. } => 1,
                    ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
                    | ActionSemanticV1::ChooseBlockerInclusion { include: false, .. } => 2,
                    ActionSemanticV1::Pass { .. } => 10,
                    ActionSemanticV1::ActivateManaAbility { .. } => 11,
                    _ => 5,
                };
                let action = decision
                    .legal_actions
                    .iter()
                    .min_by_key(|action| rank(&action.semantic))
                    .unwrap();
                session
                    .step(
                        decision.episode_id,
                        decision.step,
                        action.selected_index,
                        &action.stable_id,
                    )
                    .unwrap();
            }
            assert!(verified, "{name} kicker choice was not reached");
        }
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn foundations_session_snapshot_restores_returning_aura_choice_and_binding() {
        use crate::rl::{ActionSemanticV1, PlayerSeatV1};
        let plains = card_id_by_name("Plains").unwrap();
        let healer = card_id_by_name("Sun-Blessed Healer").unwrap();
        let aura = card_id_by_name("Bind the Monster").unwrap();
        let cards = [vec![plains; 20], vec![healer; 10], vec![aura; 10]].concat();
        let identity = content_identity(&cards);
        let mut session = RlEpisodeSessionV1::reset_with_custom_decks_v1(
            7,
            123,
            8192,
            16384,
            [identity.clone(), identity],
            [&cards, &cards],
            PriorityModeV1::EngineWindowsV1,
            true,
        )
        .unwrap();
        for _ in 0..4096 {
            let before = session.current_response();
            let RlSessionResponseV1::Decision(decision) = &before else {
                panic!("game ended before the returning Aura choice: {before:?}");
            };
            if matches!(&decision.legal_actions[0].semantic,
                ActionSemanticV1::ChooseEffectTarget { source, .. } if source.card_db_id == healer)
            {
                let snapshot = session.snapshot_v5();
                let before_hash = session.privileged_environment_hash();
                let action = decision.legal_actions.last().unwrap();
                let after = session
                    .step(
                        decision.episode_id,
                        decision.step,
                        action.selected_index,
                        &action.stable_id,
                    )
                    .unwrap();
                let after_hash = session.privileged_environment_hash();
                session.restore_v5(&snapshot);
                assert_eq!(session.current_response(), before);
                assert_eq!(session.privileged_environment_hash(), before_hash);
                assert_eq!(
                    session
                        .step(
                            decision.episode_id,
                            decision.step,
                            action.selected_index,
                            &action.stable_id
                        )
                        .unwrap(),
                    after
                );
                assert_eq!(session.privileged_environment_hash(), after_hash);
                return;
            }
            let seat = if decision.acting_player == PlayerSeatV1::P0 {
                0
            } else {
                1
            };
            let public = &decision.observation.projection.surface;
            let has_healer = public.battlefield[seat]
                .iter()
                .any(|card| card.stable.card_db_id == healer);
            let has_grave_aura = public.graveyards[seat]
                .iter()
                .any(|card| card.stable.card_db_id == aura);
            let rank = |semantic: &ActionSemanticV1| match semantic {
                ActionSemanticV1::PlayLand { .. } => 0,
                ActionSemanticV1::CastSpell { source, .. }
                    if source.card_db_id == healer && (!has_healer || has_grave_aura) =>
                {
                    1
                }
                ActionSemanticV1::ChooseKicker { pay, .. } if *pay == has_grave_aura => 0,
                ActionSemanticV1::ChooseKicker { .. } => 9,
                ActionSemanticV1::Discard { cards, .. }
                    if cards.iter().any(|card| card.card_db_id == aura) =>
                {
                    0
                }
                ActionSemanticV1::ChooseAttackerInclusion { include: false, .. }
                | ActionSemanticV1::ChooseBlockerInclusion { include: false, .. } => 2,
                ActionSemanticV1::CastSpell { .. } => 12,
                ActionSemanticV1::Pass { .. } => 10,
                ActionSemanticV1::ActivateManaAbility { .. } => 11,
                _ => 5,
            };
            let action = decision
                .legal_actions
                .iter()
                .min_by_key(|action| rank(&action.semantic))
                .unwrap();
            session
                .step(
                    decision.episode_id,
                    decision.step,
                    action.selected_index,
                    &action.stable_id,
                )
                .unwrap();
        }
        panic!("returning Aura choice was not reached");
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn foundations_session_snapshot_restores_pending_legend_choice_and_binding() {
        use crate::rl::ActionSemanticV1;
        let forest = card_id_by_name("Forest").unwrap();
        let elves = card_id_by_name("Llanowar Elves").unwrap();
        let dwynen = card_id_by_name("Dwynen, Gilt-Leaf Daen").unwrap();
        let cards = [vec![forest; 20], vec![elves; 8], vec![dwynen; 12]].concat();
        let identity = content_identity(&cards);
        let mut session = RlEpisodeSessionV1::reset_with_custom_decks_v1(
            7,
            123,
            8192,
            16384,
            [identity.clone(), identity],
            [&cards, &cards],
            PriorityModeV1::EngineWindowsV1,
            true,
        )
        .unwrap();
        for _ in 0..4096 {
            let before = session.current_response();
            let RlSessionResponseV1::Decision(decision) = &before else {
                panic!("game ended before a legend choice");
            };
            if matches!(
                &decision.legal_actions[0].semantic,
                ActionSemanticV1::ChooseLegendPermanent { .. }
            ) {
                let snapshot = session.snapshot_v5();
                let before_hash = session.privileged_environment_hash();
                let action = &decision.legal_actions[1];
                let after = session
                    .step(
                        decision.episode_id,
                        decision.step,
                        action.selected_index,
                        &action.stable_id,
                    )
                    .unwrap();
                let after_hash = session.privileged_environment_hash();
                session.restore_v5(&snapshot);
                assert_eq!(session.current_response(), before);
                assert_eq!(session.privileged_environment_hash(), before_hash);
                assert_eq!(
                    session
                        .step(
                            decision.episode_id,
                            decision.step,
                            action.selected_index,
                            &action.stable_id
                        )
                        .unwrap(),
                    after
                );
                assert_eq!(session.privileged_environment_hash(), after_hash);
                return;
            }
            let rank = |semantic: &ActionSemanticV1| match semantic {
                ActionSemanticV1::PlayLand { .. } => 0,
                ActionSemanticV1::CastSpell { .. } => 1,
                ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                | ActionSemanticV1::ChooseBlockerInclusion { include: true, .. } => 2,
                ActionSemanticV1::Pass { .. } => 10,
                ActionSemanticV1::ActivateManaAbility { .. } => 11,
                _ => 5,
            };
            let action = decision
                .legal_actions
                .iter()
                .min_by_key(|action| rank(&action.semantic))
                .unwrap();
            session
                .step(
                    decision.episode_id,
                    decision.step,
                    action.selected_index,
                    &action.stable_id,
                )
                .unwrap();
        }
        panic!("legend choice was not reached within the bound");
    }

    #[test]
    fn foundations_session_snapshot_restores_pending_damage_action_and_binding() {
        use crate::rl::ActionSemanticV1;
        let forest = card_id_by_name("Forest").unwrap();
        let elves = card_id_by_name("Llanowar Elves").unwrap();
        let paladin = card_id_by_name("Spinewoods Paladin").unwrap();
        let cards = [vec![forest; 20], vec![elves; 12], vec![paladin; 8]].concat();
        let identity = content_identity(&cards);
        let mut session = RlEpisodeSessionV1::reset_with_custom_decks_v1(
            7,
            123,
            8192,
            16384,
            [identity.clone(), identity],
            [&cards, &cards],
            PriorityModeV1::EngineWindowsV1,
            true,
        )
        .unwrap();
        for _ in 0..4096 {
            let before = session.current_response();
            let RlSessionResponseV1::Decision(decision) = &before else {
                panic!("game ended before a damage choice");
            };
            if matches!(
                &decision.legal_actions[0].semantic,
                ActionSemanticV1::ChooseCombatDamageRange { .. }
            ) {
                let snapshot = session.snapshot_v5();
                let before_hash = session.privileged_environment_hash();
                let action = &decision.legal_actions[0];
                let after = session
                    .step(
                        decision.episode_id,
                        decision.step,
                        action.selected_index,
                        &action.stable_id,
                    )
                    .unwrap();
                let after_hash = session.privileged_environment_hash();
                session.restore_v5(&snapshot);
                assert_eq!(session.current_response(), before);
                assert_eq!(session.privileged_environment_hash(), before_hash);
                assert_eq!(
                    session
                        .step(
                            decision.episode_id,
                            decision.step,
                            action.selected_index,
                            &action.stable_id
                        )
                        .unwrap(),
                    after
                );
                assert_eq!(session.privileged_environment_hash(), after_hash);
                return;
            }
            let rank = |semantic: &ActionSemanticV1| match semantic {
                ActionSemanticV1::PlayLand { .. } => 0,
                ActionSemanticV1::CastSpell { .. } => 1,
                ActionSemanticV1::ChooseAttackerInclusion { include: true, .. }
                | ActionSemanticV1::ChooseBlockerInclusion { include: true, .. } => 2,
                ActionSemanticV1::Pass { .. } => 10,
                ActionSemanticV1::ActivateManaAbility { .. } => 11,
                _ => 5,
            };
            let action = decision
                .legal_actions
                .iter()
                .min_by_key(|action| rank(&action.semantic))
                .unwrap();
            session
                .step(
                    decision.episode_id,
                    decision.step,
                    action.selected_index,
                    &action.stable_id,
                )
                .unwrap();
        }
        panic!("damage choice was not reached within the bound");
    }

    #[test]
    fn engine_priority_session_snapshot_restores_response_binding_and_next_transition() {
        let forest = card_id_by_name("Forest").unwrap();
        let island = card_id_by_name("Island").unwrap();
        let cards = [vec![forest; 40], vec![island; 40]];
        let identities = [&cards[0][..], &cards[1][..]].map(content_identity);
        let mut session = RlEpisodeSessionV1::reset_with_custom_decks_v1(
            7,
            123,
            4096,
            8192,
            identities,
            [&cards[0], &cards[1]],
            PriorityModeV1::EngineWindowsV1,
            false,
        )
        .unwrap();
        let original = session.current_response();
        let RlSessionResponseV1::Decision(decision) = &original else {
            panic!("expected priority");
        };
        assert_eq!(
            decision
                .observation
                .projection
                .surface
                .surface_context
                .engine_priority_version,
            Some(1)
        );
        let snapshot = session.snapshot_v5();
        let hash = session.privileged_environment_hash();
        let action = &decision.legal_actions[0];
        let first = session
            .step(
                decision.episode_id,
                decision.step,
                action.selected_index,
                &action.stable_id,
            )
            .unwrap();
        let after_hash = session.privileged_environment_hash();
        session.restore_v5(&snapshot);
        assert_eq!(session.current_response(), original);
        assert_eq!(session.privileged_environment_hash(), hash);
        assert_eq!(
            session
                .step(
                    decision.episode_id,
                    decision.step,
                    action.selected_index,
                    &action.stable_id
                )
                .unwrap(),
            first
        );
        assert_eq!(session.privileged_environment_hash(), after_hash);
    }
}
