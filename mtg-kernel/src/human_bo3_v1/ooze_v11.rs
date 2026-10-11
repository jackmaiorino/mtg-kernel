//! Sealed explicit Ooze label successor over the V9 public facts.
//! V10 is reserved by the separately prepared private keep-one successor.
use super::visible::{self, Handles};
use super::*;
use crate::policy_observation_v8::ObservationV8;
use crate::rl::ActionSemanticV1;
use crate::rl_session::human_v8::HumanDecisionBindingV8;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

pub struct HumanOozeSessionV11 {
    inner: FastActorSessionV1,
}
#[derive(Clone)]
pub struct HumanOozeSnapshotV11(FastActorSessionSnapshotV1);
impl HumanOozeSessionV11 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_human_v8()?,
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v11(&self) -> HumanOozeSnapshotV11 {
        HumanOozeSnapshotV11(self.inner.snapshot_v1())
    }
    pub fn restore_v11(&mut self, snapshot: &HumanOozeSnapshotV11) {
        self.inner.restore_v1(&snapshot.0);
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::ooze_public_v1::OozePublicObservationV1, crate::rl::RlContractError> {
        let observation = self
            .inner
            .human_raid_observation_v9(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale Ooze public decision".into()))?;
        crate::ooze_public_v1::from_authenticated_v9(observation)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionOozeV11 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanExtensionsV9>,
    pub actions: Vec<HumanLegalActionV1>,
}

fn project_base(
    observation: &ObservationV8,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<(HumanVisibleStateV8, Vec<HumanLegalActionV1>, Vec<u32>), Error> {
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    let mut handles = Handles::from_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_hand_cards,
        &observation.known_library_cards,
        observation.extensions.decision_local_library.as_ref(),
    )?;
    // Public stack and authenticated historical contexts establish producer
    // identities before relation or label lookup. No hidden inventory scan.
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    let initial = super::life_ward_v8::project_state(observation, &mut handles)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial.extensions.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for rows in unary.values_mut() {
        rows.sort();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = super::life_ward_v8::project_state(observation, &mut handles)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                super::labels::label_ooze_v11(action, observation, &handles, human)?,
                u32::try_from(index).map_err(|_| Error::InvalidAction)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let engine_indexes = choices.iter().map(|(_, index)| *index).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| HumanLegalActionV1 {
            action_index: index as u32,
            label,
        })
        .collect();
    Ok((state, actions, engine_indexes))
}

struct BoundOozeV11 {
    expected: FastActorDecisionV1,
    binding: HumanDecisionBindingV8,
    public: crate::ooze_public_v1::OozePublicObservationV1,
    visible: HumanDecisionOozeV11,
    engine_indexes: Vec<u32>,
}
pub struct HumanOozeProjectorV11 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundOozeV11>,
    accepted: Option<HumanActionReceiptV1>,
}
impl HumanOozeProjectorV11 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }
    pub fn project_current(
        &mut self,
        session: &HumanOozeSessionV11,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionOozeV11, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .human_current_decision_input_v8(expected, self.human_seat)
            .map_err(|_| Error::StaleDecision)?;
        let public = session
            .public_observation(expected)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.expected == expected
                && pending.binding == binding
                && pending.public == public
            {
                return Ok(pending.visible.clone());
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (old, actions, engine_indexes) = project_base(&observation, &actions, self.human_seat)?;
        let flags = public.predecessor.extensions.attacked_this_turn;
        let seat = if self.human_seat == PlayerSeatV1::P0 {
            0
        } else {
            1
        };
        let visible = HumanDecisionOozeV11 {
            schema: "mtg-kernel-human-ooze-decision/v11".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state: HumanVisibleStateV1 {
                public: old.public,
                own_hand: old.own_hand,
                known_library_cards: old.known_library_cards,
                known_hand_cards: old.known_hand_cards,
                extensions: HumanExtensionsV9 {
                    predecessor: old.extensions,
                    own_attacked_this_turn: flags[seat],
                    opponent_attacked_this_turn: flags[1 - seat],
                },
                policy_context: old.policy_context,
                known_face_down_cards: old.known_face_down_cards,
                poison_counters: old.poison_counters,
                poison_prevention: old.poison_prevention,
                creatures_attacked_this_turn: old.creatures_attacked_this_turn,
                ninja_emblems: old.ninja_emblems,
                restricted_mana: old.restricted_mana,
            },
            actions,
        };
        self.pending = Some(BoundOozeV11 {
            expected,
            binding,
            public,
            visible: visible.clone(),
            engine_indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanOozeSessionV11,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if let Some(receipt) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let pending = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if request.prompt_seq != pending.visible.prompt_seq {
            return Err(Error::StaleDecision);
        }
        let index = *pending
            .engine_indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .human_validate_bound_decision_v8(pending.expected, self.human_seat, &pending.binding)
            .map_err(|_| Error::StaleDecision)?;
        if session
            .public_observation(pending.expected)
            .map_err(|_| Error::StaleDecision)?
            != pending.public
        {
            return Err(Error::StaleDecision);
        }
        let result = session
            .inner
            .step(pending.expected.episode_id, pending.expected.step, index);
        self.pending = None;
        result.map_err(|_| Error::ExecutionFailed)?;
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
