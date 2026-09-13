//! Fixed-seat, bound human decisions for the V3 session.
//!
//! This is an in-process adapter, not a BO3 runner or a human evaluation.
//! Only `HumanDecisionV1`, `HumanActionRequestV1`, `HumanActionReceiptV1`, and
//! safe errors are transport data. The session, bindings, and source V6 input
//! remain backend-only. No opponent decision is projected to a human.

mod labels;
mod visible;
pub use visible::*;

use crate::rl::PlayerSeatV1;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1, FlatActionDecisionBindingV3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanDecisionErrorV1 {
    NotHumanTurn,
    StaleDecision,
    InvalidVisibleReference,
    UnsupportedPrompt,
    InvalidAction,
    ConflictingRetry,
    PromptSequenceExhausted,
    ExecutionFailed,
}

impl std::fmt::Display for HumanDecisionErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotHumanTurn => "Waiting for the opponent.",
            Self::StaleDecision => "This choice is no longer current. Refresh the prompt.",
            Self::InvalidVisibleReference => "The visible decision could not be represented.",
            Self::UnsupportedPrompt => {
                "This decision needs a human-readable description that is not supported yet."
            }
            Self::InvalidAction => "Choose one of the actions listed in the current prompt.",
            Self::ConflictingRetry => "That prompt already accepted a different action.",
            Self::PromptSequenceExhausted => "The prompt sequence is exhausted.",
            Self::ExecutionFailed => "The game could not apply this choice.",
        })
    }
}
impl std::error::Error for HumanDecisionErrorV1 {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanActionRequestV1 {
    pub prompt_seq: u64,
    pub action_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanActionReceiptV1 {
    pub prompt_seq: u64,
    pub action_index: u32,
}

// Deliberately neither Serialize nor Debug: no transport representation for
// the private engine binding or source decision identity exists here.
struct BoundHumanDecisionV1 {
    expected: FastActorDecisionV1,
    binding: FlatActionDecisionBindingV3,
    visible: HumanDecisionV1,
    engine_action_indexes: Vec<u32>,
}

/// One adapter belongs to one backend-owned human seat and match session.
/// Drivers must use fresh episode identities for new games, as the V3 session
/// contract requires. Callers cannot choose a new observer in a request.
pub struct HumanDecisionProjectorV1 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundHumanDecisionV1>,
    accepted: Option<HumanActionReceiptV1>,
}

impl HumanDecisionProjectorV1 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }

    /// Read-only with respect to game state and RNG. Reprinting the same
    /// validated decision preserves all prompt bytes and request identity.
    pub fn project_current(
        &mut self,
        session: &FastActorSessionV1,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionV1, HumanDecisionErrorV1> {
        if expected.acting_player != self.human_seat {
            return Err(HumanDecisionErrorV1::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .human_current_decision_input_v1(expected, self.human_seat)
            .map_err(|_| HumanDecisionErrorV1::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.expected == expected && pending.binding == binding {
                return Ok(pending.visible.clone());
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(HumanDecisionErrorV1::PromptSequenceExhausted)?;
        let (visible, engine_action_indexes) = visible::project_decision(
            &observation,
            &actions,
            self.human_seat,
            self.next_prompt_seq,
        )?;
        self.pending = Some(BoundHumanDecisionV1 {
            expected,
            binding,
            visible: visible.clone(),
            engine_action_indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }

    /// Accept exactly one listed action at the private current V3 binding.
    /// An immediate identical retry returns its receipt without applying it
    /// twice. Receipts contain no next-actor observation or terminal detail.
    /// A future transport driver must journal accepted commands durably; this
    /// in-memory receipt cache does not implement crash recovery.
    pub fn submit(
        &mut self,
        session: &mut FastActorSessionV1,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, HumanDecisionErrorV1> {
        if let Some(receipt) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(HumanDecisionErrorV1::ConflictingRetry)
                };
            }
        }
        let pending = self
            .pending
            .as_ref()
            .ok_or(HumanDecisionErrorV1::StaleDecision)?;
        if pending.visible.prompt_seq != request.prompt_seq {
            return Err(HumanDecisionErrorV1::StaleDecision);
        }
        if request.action_index as usize >= pending.visible.actions.len() {
            return Err(HumanDecisionErrorV1::InvalidAction);
        }
        session
            .flat_policy_validate_cached_binding_v3(pending.expected, pending.binding)
            .map_err(|_| HumanDecisionErrorV1::StaleDecision)?;
        let engine_index = pending.engine_action_indexes[request.action_index as usize];
        let result = session.consume_current_flat_action_slice_v3(pending.binding, engine_index);
        self.pending = None;
        result.map_err(|_| HumanDecisionErrorV1::ExecutionFailed)?;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some(receipt.clone());
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
