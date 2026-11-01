//! The payload the producer hands the search entrant, and its import.
//!
//! Everything in the envelope is public or V's own: the deck identities and
//! limits, V's seat and decision index, the handle labels and the canonical
//! state. The import rebuilds a flat-action session (contract v3, the
//! bridge's) around the state.

use super::{InfosetV1, INFOSET_SCHEMA_V1};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadV1 {
    pub schema: String,
    /// SHA-256 of the canonical bytes (`InfosetV1::bytes`).
    pub sha256: String,
    pub deck_ids: [String; 2],
    pub viewer: u8,
    pub own_decision_index: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub handles: Vec<String>,
    pub state: serde_json::Value,
}

impl PayloadV1 {
    pub(crate) fn new(
        export: &InfosetV1,
        live: &FastActorSessionV1,
        viewer: PlayerId,
        own_decision_index: u64,
    ) -> Self {
        let (max_physical_decisions, max_policy_steps) = live.limits_v1();
        Self {
            schema: INFOSET_SCHEMA_V1.into(),
            sha256: export.sha256.clone(),
            deck_ids: live.deck_ids_v1().clone(),
            viewer: viewer.0,
            own_decision_index,
            max_physical_decisions,
            max_policy_steps,
            handles: export.handles.iter().map(super::HandleV1::label).collect(),
            state: serde_json::to_value(&export.state).expect("state serializes"),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("payload serializes")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let p: Self = serde_json::from_slice(bytes).map_err(|e| format!("payload: {e}"))?;
        if p.schema != INFOSET_SCHEMA_V1 {
            return Err(format!("payload schema {}", p.schema));
        }
        Ok(p)
    }

    /// The imported session. Fails closed on any malformed field.
    pub(crate) fn import(&self) -> Result<FastActorSessionV1, String> {
        let state: crate::state::GameState = serde_json::from_value(self.state.clone())
            .map_err(|e| format!("payload state: {e}"))?;
        let template = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            0,
            0,
            self.max_physical_decisions,
            self.max_policy_steps,
            self.deck_ids.clone(),
        )
        .map_err(|e| format!("template session: {}", e.message))?;
        Ok(FastActorSessionV1::import_infoset_v1(
            &template,
            state,
            self.own_decision_index,
        ))
    }
}
