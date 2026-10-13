//! Fixed-seat, bound human decisions with explicit V3 or V4 validation.
//!
//! This is an in-process adapter, not a BO3 runner or a human evaluation.
//! Only `HumanDecisionV1`, `HumanActionRequestV1`, `HumanActionReceiptV1`, and
//! safe errors are transport data. The session, bindings, and source V6 input
//! remain backend-only. No opponent decision is projected to a human.

mod heartfire_history_v1;
mod heartfire_v12;
pub use heartfire_history_v1::*;
mod bloom_tender_v24;
mod employment_v18;
mod labels;
mod librarian_v20;
pub use librarian_v20::*;
mod life_ward_v8;
pub use bloom_tender_v24::*;
mod ooze_v11;
pub use employment_v18::*;
mod sower_v14;
pub use sower_v14::*;
mod wildwood_v16;
pub use wildwood_v16::*;
mod cerberus_v19;
mod raid_entry_v21;
pub use cerberus_v19::*;
pub use raid_entry_v21::*;
pub mod extra_turn_v23;
pub use extra_turn_v23::*;
mod juggernaut_v25;
pub use juggernaut_v25::*;
mod ashroot_v27;
pub use ashroot_v27::*;
mod stromkirk_v28;
pub use stromkirk_v28::*;
mod crystal_v22;
pub use crystal_v22::*;
mod arahbo_v31;
mod authority_v17;
mod garruk_v30;
pub use arahbo_v31::*;
pub use authority_v17::*;
mod gourmand_v29;
pub use garruk_v30::*;
pub use gourmand_v29::{
    HumanDecisionGourmandV29, HumanGourmandExtensionsV29, HumanGourmandProjectorV29,
    HumanGourmandResolutionV29, HumanGourmandRestrictionV29, HumanGourmandSacrificeMenuV29,
    HumanGourmandSessionV29, HumanGourmandSnapshotV29,
};

mod frenzied_v15;
pub use frenzied_v15::*;
mod raid_v9;
pub mod variable_cast_v1;
pub use ooze_v11::*;
mod soul_shackled_zombie_v13;
pub use heartfire_v12::*;
mod visible;
pub use life_ward_v8::*;
pub use raid_v9::*;
pub use soul_shackled_zombie_v13::*;
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
    use_v4_binding: bool,
    next_prompt_seq: u64,
    pending: Option<BoundHumanDecisionV1>,
    accepted: Option<HumanActionReceiptV1>,
}

/// Offline projection of recorded actor-visible data through the same menu
/// projection used by the live adapter. This creates no live action binding
/// and does not prove that the supplied record came from a legal engine state.
pub(crate) fn project_recorded_decision_v1(
    observation: &crate::policy_observation_v6::ObservationV6,
    actions: &[crate::rl::ActionSemanticV1],
    human: PlayerSeatV1,
    prompt_seq: u64,
    selected_engine_index: usize,
) -> Result<(HumanDecisionV1, u32), HumanDecisionErrorV1> {
    if selected_engine_index >= actions.len() {
        return Err(HumanDecisionErrorV1::InvalidAction);
    }
    let selected_engine_index =
        u32::try_from(selected_engine_index).map_err(|_| HumanDecisionErrorV1::InvalidAction)?;
    let (visible, engine_indexes) =
        visible::project_decision(observation, actions, human, prompt_seq)?;
    let public_index = engine_indexes
        .iter()
        .position(|index| *index == selected_engine_index)
        .ok_or(HumanDecisionErrorV1::InvalidAction)?;
    let public_index =
        u32::try_from(public_index).map_err(|_| HumanDecisionErrorV1::InvalidAction)?;
    Ok((visible, public_index))
}

impl HumanDecisionProjectorV1 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            use_v4_binding: false,
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }

    /// Explicit opt-in for runtimes whose playing policy uses V4 references.
    /// Legacy constructors continue validating the original V3 cache.
    pub(crate) fn new_v4(human_seat: PlayerSeatV1) -> Self {
        Self {
            use_v4_binding: true,
            ..Self::new(human_seat)
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
        let (observation, actions, binding) = if self.use_v4_binding {
            session.human_current_decision_input_v4(expected, self.human_seat)
        } else {
            session.human_current_decision_input_v1(expected, self.human_seat)
        }
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

    /// Accept exactly one listed action at the private current binding.
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
        let engine_index = pending.engine_action_indexes[request.action_index as usize];
        let result = if self.use_v4_binding {
            // V4 has no V3 cache. Revalidate the exact expected decision, origin,
            // legal menu and commitment before applying its private engine index.
            let (_, _, current_binding) = session
                .human_current_decision_input_v4(pending.expected, self.human_seat)
                .map_err(|_| HumanDecisionErrorV1::StaleDecision)?;
            if current_binding != pending.binding {
                return Err(HumanDecisionErrorV1::StaleDecision);
            }
            session.step(
                pending.expected.episode_id,
                pending.expected.step,
                engine_index,
            )
        } else {
            session
                .flat_policy_validate_cached_binding_v3(pending.expected, pending.binding)
                .map_err(|_| HumanDecisionErrorV1::StaleDecision)?;
            session.consume_current_flat_action_slice_v3(pending.binding, engine_index)
        };
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

mod inspiration_v37;
mod twinflame_v38;
pub use inspiration_v37::*;
pub use twinflame_v38::{
    HumanDecisionTwinflameV38, HumanTwinflameExtensionsV38, HumanTwinflameProjectorV38,
    HumanTwinflameSessionV38, HumanTwinflameSnapshotV38,
};
mod banner_v36;
mod fiery_v33;
mod zul_ashur_v35;
pub use banner_v36::*;
pub use fiery_v33::*;
mod fishing_pole_v39;
mod krenko_v34;
pub use fishing_pole_v39::*;
pub use krenko_v34::*;
pub use zul_ashur_v35::*;

mod strongbox_v32;
pub use strongbox_v32::*;

mod banishing_v26;
pub use banishing_v26::*;

mod akroma_early_v40;
mod akroma_families_v40;
pub use akroma_early_v40::{HumanEarlyV40, HumanZombieContextV40};
mod akroma_v40;
mod game_outcome_family_v40;
mod game_outcome_v41;
mod limited_common_v1;
pub use akroma_families_v40::HumanFamilyV40;
pub use akroma_v40::{
    HumanAkromaExtensionsV40, HumanAkromaGrantV40, HumanAkromaProjectorV40, HumanAkromaSessionV40,
    HumanAkromaSnapshotV40, HumanColorDepartureV40, HumanDecisionAkromaV40,
};
pub use game_outcome_family_v40::{HumanGameOutcomeFamilyV40, HumanGameOutcomeRulesV40};
pub use game_outcome_v41::{
    HumanDecisionGameOutcomeV41, HumanGameOutcomeExtensionsV41, HumanGameOutcomeProjectorV41,
    HumanGameOutcomeSessionV41, HumanGameOutcomeSnapshotV41, HumanGameOutcomeSourceV41,
};
pub(crate) use limited_common_v1::project_limited_common_v1;
pub use limited_common_v1::{LimitedCombatAssignmentV1, LimitedCombatV1, LimitedCommonDecisionV1};

pub mod ajani_v42;
pub use ajani_v42::{
    HumanAjaniProjectorV42, HumanAjaniSessionV42, HumanAjaniSnapshotV42, HumanDecisionAjaniV42,
};

#[cfg(test)]
mod tests;

pub mod divine_v44;
pub mod faebloom_v43;
pub use divine_v44::{
    HumanDecisionDivineV44, HumanDivineProjectorV44, HumanDivineSessionV44, HumanDivineSnapshotV44,
};
pub use faebloom_v43::{
    HumanDecisionFaebloomV43, HumanFaebloomProjectorV43, HumanFaebloomSessionV43,
    HumanFaebloomSnapshotV43,
};
