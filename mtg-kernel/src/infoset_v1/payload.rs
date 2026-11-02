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
    /// SHA-256 of the whole envelope with this field empty: the search
    /// input the receipts name.
    pub sha256: String,
    /// SHA-256 of the canonical state bytes (`InfosetV1::sha256`).
    pub state_sha256: String,
    pub deck_ids: [String; 2],
    pub viewer: u8,
    pub own_decision_index: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
    pub handles: Vec<String>,
    /// The live session surface's public context (combat priority).
    pub surface: crate::policy_surface_v5::SurfaceContextV1,
    pub state: serde_json::Value,
}

impl PayloadV1 {
    /// None while the live surface names objects (a combat scan or reshape
    /// in progress): such a decision is refused (R2) by the caller.
    pub(crate) fn new(
        export: &InfosetV1,
        live: &FastActorSessionV1,
        viewer: PlayerId,
        own_decision_index: u64,
    ) -> Option<Self> {
        let (max_physical_decisions, max_policy_steps) = live.limits_v1();
        let surface = live.surface_context_v1()?;
        let mut payload = Self {
            schema: INFOSET_SCHEMA_V1.into(),
            sha256: String::new(),
            state_sha256: export.sha256.clone(),
            deck_ids: live.deck_ids_v1().clone(),
            viewer: viewer.0,
            own_decision_index,
            max_physical_decisions,
            max_policy_steps,
            handles: export.handles.iter().map(super::HandleV1::label).collect(),
            surface,
            state: serde_json::to_value(&export.state).expect("state serializes"),
        };
        payload.sha256 = payload.envelope_digest();
        Some(payload)
    }

    /// The digest of every field but `sha256`.
    pub(crate) fn envelope_digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut unsigned = self.clone();
        unsigned.sha256 = String::new();
        format!("{:x}", Sha256::digest(unsigned.to_bytes()))
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

    /// The imported session. Fails closed on any malformed field, and on a
    /// digest that does not match the canonical bytes it imports.
    pub(crate) fn import(&self) -> Result<FastActorSessionV1, String> {
        if self.schema != INFOSET_SCHEMA_V1 {
            return Err(format!("payload schema {}", self.schema));
        }
        if self.viewer > 1 {
            return Err("payload viewer".into());
        }
        let state: crate::state::GameState = serde_json::from_value(self.state.clone())
            .map_err(|e| format!("payload state: {e}"))?;
        if state.objects.len() != self.handles.len() {
            return Err("payload handles do not cover the arena".into());
        }
        let digest = super::export::canonical_digest_v1(&state, &self.handles);
        if digest != self.state_sha256 {
            return Err("payload state digest differs from its canonical bytes".into());
        }
        if self.envelope_digest() != self.sha256 {
            return Err("payload digest differs from its envelope".into());
        }
        let template = FastActorSessionV1::reset_with_decks_and_limits_flat_action_v3(
            0,
            0,
            self.max_physical_decisions,
            self.max_policy_steps,
            self.deck_ids.clone(),
        )
        .map_err(|e| format!("template session: {}", e.message))?;
        Ok(FastActorSessionV1::import_infoset_with_surface_v1(
            &template,
            state,
            self.own_decision_index,
            Some(self.surface),
        ))
    }
}
