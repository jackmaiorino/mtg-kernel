//! The payload the producer hands the search entrant, and its import.
//!
//! Everything in the envelope is public or V's own: the deck identities and
//! limits, V's seat and decision index, the handle labels and the canonical
//! state. The import rebuilds a flat-action session (contract v3, the
//! bridge's) around the state.

use super::{InfosetV1, INFOSET_SCHEMA_V1};
use crate::ids::PlayerId;
use crate::rl_session::FastActorSessionV1;
use crate::state::{GameState, Zone};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
    /// The live session surface's public context (combat priority, and a
    /// live combat scan's record).
    pub surface: crate::policy_surface_v5::SurfaceContextV1,
    pub state: serde_json::Value,
    /// The selected viewer's opponent-hand minima in public name terms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reveal_hand: Option<Vec<RevealHandMinimumV4>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevealHandMinimumV4 {
    card_name: String,
    minimum: usize,
}

fn reveal_hand_v4(state: &GameState, viewer: PlayerId) -> Option<Vec<RevealHandMinimumV4>> {
    let knowledge = &state.reveal_knowledge_v4.as_ref()?[viewer.index()];
    let mut facts = knowledge
        .hand(viewer.opponent())
        .map(|(def, minimum)| {
            Some(RevealHandMinimumV4 {
                card_name: crate::card_def::CARD_DEFS.get(usize::from(def))?.name.into(),
                minimum,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    facts.sort_unstable_by(|a, b| a.card_name.cmp(&b.card_name));
    Some(facts)
}

pub(super) fn validate_reveal_knowledge_v4(state: &GameState, viewer: PlayerId) -> Result<(), String> {
    let knowledge = state
        .reveal_knowledge_v4
        .as_ref()
        .ok_or("missing reveal-v4 knowledge")?;
    if state.hand_knowledge_by_name_v2 || state.hand_departures_by_name_v1 {
        return Err("reveal-v4 payload carries a legacy hand mode".into());
    }
    for observer in [PlayerId::P0, PlayerId::P1] {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let mut expected = knowledge[observer.index()].hand(owner).peekable();
            let rows = state.known_hand_cards(observer, owner);
            if observer != viewer || owner == viewer {
                if expected.peek().is_some() || !rows.is_empty() {
                    return Err("reveal-v4 payload carries out-of-view hand facts".into());
                }
                continue;
            }
            let mut names = BTreeMap::<u16, usize>::new();
            let mut seen = BTreeSet::new();
            for row in rows {
                let live = state.objects.try_get(row.object).filter(|card| {
                    card.owner == owner
                        && card.zone == Zone::Hand
                        && !card.v4.is_token
                        && card.spell_copy_origin.is_none()
                        && card.zone_change_count == row.zone_change_count
                        && state.players[owner.index()].hand.contains(&row.object)
                });
                let card = live.ok_or("reveal-v4 payload carries a stale hand representative")?;
                if !seen.insert(row.object) {
                    return Err("reveal-v4 payload repeats a hand representative".into());
                }
                *names.entry(card.card_def).or_default() += 1;
            }
            if !expected.eq(names) {
                return Err("reveal-v4 payload hand rows differ from its public minima".into());
            }
        }
    }
    Ok(())
}

impl PayloadV1 {
    /// None while the live surface names objects the payload cannot carry
    /// (a Hunter trigger order, a discard or optional-cost reshape, or a
    /// combat object without a canonical index): the caller refuses (R2). A
    /// combat scan travels as the context's combat record (amendment 43.1).
    pub fn new(
        export: &InfosetV1,
        live: &FastActorSessionV1,
        viewer: PlayerId,
        own_decision_index: u64,
    ) -> Option<Self> {
        let (max_physical_decisions, max_policy_steps) = live.limits_v1();
        let surface = live.payload_surface_context_v1(&super::order::canonical_ids_v1(export))?;
        // A discard record carries only the viewer's own discard (the import
        // refuses any other); refuse it here too.
        if surface.discard.as_ref().is_some_and(|d| d.player != viewer) {
            return None;
        }
        // Likewise an optional-cost record (amendment 43.9): only the
        // viewer's own reshape.
        if surface
            .optional_cost
            .as_ref()
            .is_some_and(|o| o.player != viewer)
        {
            return None;
        }
        let mut payload = Self {
            schema: export.schema.into(),
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
            reveal_hand: if export.schema == super::INFOSET_SCHEMA_REVEAL_V4 {
                Some(reveal_hand_v4(&export.state, viewer)?)
            } else {
                None
            },
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
        if !matches!(
            p.schema.as_str(),
            INFOSET_SCHEMA_V1
                | super::INFOSET_SCHEMA_V2
                | super::INFOSET_SCHEMA_V3
                | super::INFOSET_SCHEMA_REVEAL_V4
        ) {
            return Err(format!("payload schema {}", p.schema));
        }
        Ok(p)
    }

    /// The imported session. Fails closed on any malformed field, and on a
    /// digest that does not match the canonical bytes it imports.
    pub fn import(&self) -> Result<FastActorSessionV1, String> {
        if !matches!(
            self.schema.as_str(),
            INFOSET_SCHEMA_V1
                | super::INFOSET_SCHEMA_V2
                | super::INFOSET_SCHEMA_V3
                | super::INFOSET_SCHEMA_REVEAL_V4
        ) {
            return Err(format!("payload schema {}", self.schema));
        }
        if self.viewer > 1 {
            return Err("payload viewer".into());
        }
        let state: crate::state::GameState = serde_json::from_value(self.state.clone())
            .map_err(|e| format!("payload state: {e}"))?;
        if self.schema == super::INFOSET_SCHEMA_REVEAL_V4 {
            let viewer = PlayerId(self.viewer);
            validate_reveal_knowledge_v4(&state, viewer)?;
            let expected = reveal_hand_v4(&state, viewer).ok_or("invalid reveal-v4 name ledger")?;
            if self.reveal_hand.as_ref() != Some(&expected) {
                return Err("reveal-v4 public name minima differ from its reducer state".into());
            }
        } else if state.reveal_knowledge_v4.is_some() || self.reveal_hand.is_some() {
            return Err("legacy infoset carries reveal-v4 knowledge".into());
        }
        if state.objects.len() != self.handles.len() {
            return Err("payload handles do not cover the arena".into());
        }
        if let Some(combat) = &self.surface.combat {
            combat.validate_in_state(&state, &self.handles)?;
        }
        if let Some(discard) = &self.surface.discard {
            if self.surface.combat.is_some() {
                return Err("payload carries both a combat and a discard record".into());
            }
            discard.validate_in_state(&state, &self.handles, PlayerId(self.viewer))?;
        }
        // Amendment 43.9: the optional-cost and madness records.
        if let Some(o) = &self.surface.optional_cost {
            if self.surface.combat.is_some() || self.surface.discard.is_some() {
                return Err("payload carries an optional-cost record beside another".into());
            }
            crate::policy_surface_v5::validate_optional_cost_in_state_v1(
                o,
                &state,
                PlayerId(self.viewer),
            )?;
        }
        if let Some(card) = self.surface.madness_reprompt {
            if self.surface.combat.is_some() {
                return Err("payload carries a madness record beside a combat record".into());
            }
            crate::policy_surface_v5::validate_madness_reprompt_in_state_v1(
                card,
                &state,
                &self.handles,
            )?;
        }
        let digest =
            super::export::canonical_digest_with_schema(&state, &self.handles, &self.schema);
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
        let session = FastActorSessionV1::import_infoset_with_surface_v1(
            &template,
            state,
            self.own_decision_index,
            Some(self.surface.clone()),
        );
        session.validate_restored_combat_scan_v1()?;
        Ok(session)
    }
}
