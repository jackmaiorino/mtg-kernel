//! Sealed Raid entry successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::raid_entry_public_v1::RaidEntryObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::raid_entry_v21::RaidEntryDecisionBindingV21;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV21 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV21 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV21 {}
pub struct HumanRaidEntrySessionV21 {
    inner: FastActorSessionV1,
    authority: AuthorityV21,
}
#[derive(Clone)]
pub struct HumanRaidEntrySnapshotV21 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanRaidEntrySessionV21 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_raid_entry_v21()?,
            authority: AuthorityV21 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v21(&self) -> HumanRaidEntrySnapshotV21 {
        HumanRaidEntrySnapshotV21 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v21(&mut self, snapshot: &HumanRaidEntrySnapshotV21) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner.restore_v1(&snapshot.snapshot);
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::raid_entry_public_v1::RaidEntryObservationV1, crate::rl::RlContractError>
    {
        self.inner
            .raid_entry_current_decision_input_v21(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale RaidEntry public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionRaidEntryV21 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanRaidEntryExtensionsV21>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanRaidEntryExtensionsV21 {
    pub predecessor: HumanExtensionsV9,
    pub keep_one: Option<crate::raid_entry_public_v1::KeepOneContextV1>,
}
fn parts(observation: &RaidEntryObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
    let base = &observation.base;
    super::life_ward_v8::ProjectionPartsV8 {
        projection: &base.projection,
        own_hand: &base.own_hand,
        known_library_cards: &base.known_library_cards,
        known_hand_cards: &base.known_hand_cards,
        known_face_down_cards: &base.known_face_down_cards,
        extensions: &base.extensions.predecessor,
    }
}
fn state(
    observation: &RaidEntryObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanRaidEntryExtensionsV21>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = usize::from(human == PlayerSeatV1::P1);
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        known_face_down_cards: old.known_face_down_cards,
        policy_context: old.policy_context,
        poison_counters: old.poison_counters,
        poison_prevention: old.poison_prevention,
        creatures_attacked_this_turn: old.creatures_attacked_this_turn,
        ninja_emblems: old.ninja_emblems,
        restricted_mana: old.restricted_mana,
        extensions: HumanRaidEntryExtensionsV21 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.base.extensions.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.base.extensions.attacked_this_turn
                    [1 - seat],
            },
            keep_one: observation.keep_one.clone(),
        },
    })
}
type ProjectedRaidEntryV21 = (
    HumanVisibleStateV1<HumanRaidEntryExtensionsV21>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);
fn project_base(
    observation: &RaidEntryObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedRaidEntryV21, Error> {
    if observation.base.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    observation
        .validate_artifact()
        .map_err(|_| Error::InvalidVisibleReference)?;
    let base = &observation.base;
    let mut handles = Handles::from_parts(
        &base.projection,
        &base.own_hand,
        &base.known_hand_cards,
        &base.known_library_cards,
        base.extensions.predecessor.decision_local_library.as_ref(),
    )?;
    for row in &base.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &base.extensions.predecessor.historical_public_sources {
        handles.insert(&row.source)?;
    }
    let initial = state(observation, &mut handles, human)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial
        .extensions
        .predecessor
        .predecessor
        .ward_life_card_traits
    {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for rows in unary.values_mut() {
        rows.sort();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = state(observation, &mut handles, human)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                super::labels::label_parts_v8(action, &parts(observation), &handles, human)?,
                u32::try_from(index).map_err(|_| Error::InvalidAction)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|rows| rows[0].0 == rows[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let indexes = choices.iter().map(|(_, index)| *index).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| HumanLegalActionV1 {
            action_index: index as u32,
            label,
        })
        .collect();
    Ok((state, actions, indexes))
}

struct BoundRaidEntryV21 {
    authority: AuthorityV21,
    expected: FastActorDecisionV1,
    binding: RaidEntryDecisionBindingV21,
    public: crate::raid_entry_public_v1::RaidEntryObservationV1,
    visible: HumanDecisionRaidEntryV21,
    engine_indexes: Vec<u32>,
}
pub struct HumanRaidEntryProjectorV21 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundRaidEntryV21>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV21)>,
}
impl HumanRaidEntryProjectorV21 {
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
        session: &HumanRaidEntrySessionV21,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionRaidEntryV21, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .raid_entry_current_decision_input_v21(expected, self.human_seat)
            .map_err(|_| Error::StaleDecision)?;
        let public = session
            .public_observation(expected)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.expected == expected
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
        let (state, actions, engine_indexes) =
            project_base(&observation, &actions, self.human_seat)?;
        let visible = HumanDecisionRaidEntryV21 {
            schema: "mtg-kernel-human-raid_entry-decision/v21".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundRaidEntryV21 {
            authority: session.authority.clone(),
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
        session: &mut HumanRaidEntrySessionV21,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if let Some((receipt, authority)) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                if *authority != session.authority {
                    return Err(Error::StaleDecision);
                }
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let pending = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if pending.authority != session.authority
            || request.prompt_seq != pending.visible.prompt_seq
        {
            return Err(Error::StaleDecision);
        }
        let index = *pending
            .engine_indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .raid_entry_validate_bound_decision_v21(
                pending.expected,
                self.human_seat,
                &pending.binding,
            )
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
        self.accepted = Some((receipt.clone(), session.authority.clone()));
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests;
