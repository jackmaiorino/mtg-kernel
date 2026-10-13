//! Sealed Wildwood successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::rl::ActionSemanticV1;
use crate::rl_session::wildwood_v16::WildwoodDecisionBindingV16;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use crate::wildwood_public_v1::WildwoodObservationV1;
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV16 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV16 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV16 {}
pub struct HumanWildwoodSessionV16 {
    inner: FastActorSessionV1,
    authority: AuthorityV16,
}
#[derive(Clone)]
pub struct HumanWildwoodSnapshotV16 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanWildwoodSessionV16 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_wildwood_v16()?,
            authority: AuthorityV16 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v16(&self) -> HumanWildwoodSnapshotV16 {
        HumanWildwoodSnapshotV16 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v16(&mut self, snapshot: &HumanWildwoodSnapshotV16) -> Result<(), Error> {
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
    ) -> Result<crate::wildwood_public_v1::WildwoodObservationV1, crate::rl::RlContractError> {
        self.inner
            .wildwood_current_decision_input_v16(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Wildwood public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionWildwoodV16 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanWildwoodExtensionsV16>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanWildwoodOccurrenceV16 {
    pub recipient_at_event: HumanCardRefV1,
    pub observer_at_event: HumanCardRefV1,
    pub placing_player: PlayerSeatV1,
    pub amount: i32,
    pub event_order: u64,
    pub captured_controller: PlayerSeatV1,
    pub original_source_present: bool,
    pub recipient_was_creature: bool,
    pub recipient_was_hydra: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanWildwoodExtensionsV16 {
    pub predecessor: HumanExtensionsV9,
    pub counter_occurrences: Vec<HumanWildwoodOccurrenceV16>,
}
fn parts(observation: &WildwoodObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
    super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    }
}
fn state(
    observation: &WildwoodObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanWildwoodExtensionsV16>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let counter_occurrences = observation
        .counter_occurrences
        .iter()
        .map(|row| {
            Ok(HumanWildwoodOccurrenceV16 {
                recipient_at_event: handles.get(&row.recipient_at_event)?.clone(),
                observer_at_event: handles.get(&row.observer_at_event)?.clone(),
                placing_player: row.placing_player,
                amount: row.amount,
                event_order: row.event_order,
                captured_controller: row.captured_controller,
                original_source_present: row.original_source_present,
                recipient_was_creature: row.recipient_was_creature,
                recipient_was_hydra: row.recipient_was_hydra,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanWildwoodExtensionsV16 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            counter_occurrences,
        },
        policy_context: old.policy_context,
        known_face_down_cards: old.known_face_down_cards,
        poison_counters: old.poison_counters,
        poison_prevention: old.poison_prevention,
        creatures_attacked_this_turn: old.creatures_attacked_this_turn,
        ninja_emblems: old.ninja_emblems,
        restricted_mana: old.restricted_mana,
    })
}
type ProjectedWildwoodV16 = (
    HumanVisibleStateV1<HumanWildwoodExtensionsV16>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

// V40 uses the same typed safe fields; no predecessor artifact/input admission.
pub(super) fn register_for_akroma_parts(
    observation: &crate::wildwood_public_v1::WildwoodObservationV1,
    handles: &mut Handles,
) -> Result<(), Error> {
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.counter_occurrences {
        handles.insert(&row.recipient_at_event)?;
        handles.insert(&row.observer_at_event)?;
    }
    Ok(())
}
pub(super) fn state_for_akroma_parts(
    observation: &crate::wildwood_public_v1::WildwoodObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanWildwoodExtensionsV16, Error> {
    Ok(state(observation, handles, human)?.extensions)
}
pub(super) fn label_for_akroma_parts(
    action: &ActionSemanticV1,
    observation: &crate::wildwood_public_v1::WildwoodObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}

pub(super) fn unary_for_akroma_parts(
    extension: &HumanWildwoodExtensionsV16,
    unary: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &extension.predecessor.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.counter_occurrences {
        unary
            .entry(row.recipient_at_event.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    Ok(())
}
fn project_base(
    observation: &WildwoodObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedWildwoodV16, Error> {
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    observation
        .validate_artifact()
        .map_err(|_| Error::InvalidVisibleReference)?;
    let mut handles = Handles::from_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_hand_cards,
        &observation.known_library_cards,
        observation.extensions.decision_local_library.as_ref(),
    )?;
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.counter_occurrences {
        handles.insert(&row.recipient_at_event)?;
        handles.insert(&row.observer_at_event)?;
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
    for row in &initial.extensions.counter_occurrences {
        unary
            .entry(row.recipient_at_event.handle.clone())
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
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
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

struct BoundWildwoodV16 {
    authority: AuthorityV16,
    expected: FastActorDecisionV1,
    binding: WildwoodDecisionBindingV16,
    public: crate::wildwood_public_v1::WildwoodObservationV1,
    visible: HumanDecisionWildwoodV16,
    engine_indexes: Vec<u32>,
}
pub struct HumanWildwoodProjectorV16 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundWildwoodV16>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV16)>,
}
impl HumanWildwoodProjectorV16 {
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
        session: &HumanWildwoodSessionV16,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionWildwoodV16, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .wildwood_current_decision_input_v16(expected, self.human_seat)
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
        let visible = HumanDecisionWildwoodV16 {
            schema: "mtg-kernel-human-wildwood-decision/v16".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundWildwoodV16 {
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
        session: &mut HumanWildwoodSessionV16,
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
            .wildwood_validate_bound_decision_v16(
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
