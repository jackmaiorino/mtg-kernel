//! Sealed Authority successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::authority_public_v1::AuthorityObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::authority_v17::AuthorityDecisionBindingV17;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct OwnerV17 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for OwnerV17 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for OwnerV17 {}
pub struct HumanAuthoritySessionV17 {
    inner: FastActorSessionV1,
    authority: OwnerV17,
}
#[derive(Clone)]
pub struct HumanAuthoritySnapshotV17 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanAuthoritySessionV17 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_authority_v17()?,
            authority: OwnerV17 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v17(&self) -> HumanAuthoritySnapshotV17 {
        HumanAuthoritySnapshotV17 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v17(&mut self, snapshot: &HumanAuthoritySnapshotV17) -> Result<(), Error> {
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
    ) -> Result<crate::authority_public_v1::AuthorityObservationV1, crate::rl::RlContractError>
    {
        self.inner
            .authority_current_decision_input_v17(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Authority public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionAuthorityV17 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanAuthorityExtensionsV17>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAuthorityOccurrenceV17 {
    pub entrant_at_entry: HumanCardRefV1,
    pub entrant_face_index: u8,
    pub observer_at_entry: HumanCardRefV1,
    pub beneficiary: PlayerSeatV1,
    pub event_order: u64,
    pub lifecycle: crate::authority_public_v1::EntryLifecycleV1,
    pub stack_index: Option<u32>,
    pub pending_order_index: Option<u32>,
    pub original_source_present: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAuthoritySourceV17 {
    pub source: HumanCardRefV1,
    pub affected_entry_controller: PlayerSeatV1,
    pub opposing_creatures_enter_tapped: bool,
    pub mandatory_gain_per_entry: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAuthorityExtensionsV17 {
    pub predecessor: HumanExtensionsV9,
    pub entry_occurrences: Vec<HumanAuthorityOccurrenceV17>,
    pub active_sources: Vec<HumanAuthoritySourceV17>,
}

fn parts(observation: &AuthorityObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &AuthorityObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanAuthorityExtensionsV17>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let mut entry_occurrences = observation
        .entry_occurrences
        .iter()
        .map(|row| {
            Ok(HumanAuthorityOccurrenceV17 {
                entrant_at_entry: handles.get(&row.entrant_at_entry)?.clone(),
                entrant_face_index: row.entrant_face_index,
                observer_at_entry: handles.get(&row.observer_at_entry)?.clone(),
                beneficiary: row.beneficiary,
                event_order: row.event_order,
                lifecycle: row.lifecycle,
                stack_index: row.stack_index,
                pending_order_index: row.pending_order_index,
                original_source_present: row.original_source_present,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    entry_occurrences.sort_by(|a, b| {
        (a.event_order, &a.observer_at_entry.handle)
            .cmp(&(b.event_order, &b.observer_at_entry.handle))
    });
    let mut active_sources = observation
        .active_sources
        .iter()
        .map(|row| {
            Ok(HumanAuthoritySourceV17 {
                source: handles.get(&row.source)?.clone(),
                affected_entry_controller: row.affected_entry_controller,
                opposing_creatures_enter_tapped: row.opposing_creatures_enter_tapped,
                mandatory_gain_per_entry: row.mandatory_gain_per_entry,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    active_sources.sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanAuthorityExtensionsV17 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            entry_occurrences,
            active_sources,
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
type ProjectedAuthorityV17 = (
    HumanVisibleStateV1<HumanAuthorityExtensionsV17>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &AuthorityObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedAuthorityV17, Error> {
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
    for row in &observation.entry_occurrences {
        handles.insert(&row.entrant_at_entry)?;
        handles.insert(&row.observer_at_entry)?;
    }
    for row in &observation.active_sources {
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
    for row in &initial.extensions.entry_occurrences {
        unary
            .entry(row.entrant_at_entry.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &initial.extensions.active_sources {
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
                label(action, observation, &handles, human)?,
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

fn label(
    action: &ActionSemanticV1,
    observation: &AuthorityObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    let ActionSemanticV1::OrderTriggers {
        actor,
        pending_sources,
        order,
    } = action
    else {
        return super::labels::label_parts_v8(action, &parts(observation), handles, human);
    };
    if !observation
        .entry_occurrences
        .iter()
        .any(|row| row.pending_order_index.is_some())
    {
        return super::labels::label_parts_v8(action, &parts(observation), handles, human);
    }
    let pending = &observation
        .projection
        .surface
        .engine_context
        .pending_triggers;
    let mut permutation = order.clone();
    permutation.sort_unstable();
    if *actor != human
        || pending_sources.len() < 2
        || pending_sources.len() != order.len()
        || permutation != (0..order.len()).collect::<Vec<_>>()
        || pending.len() < order.len()
    {
        return Err(Error::UnsupportedPrompt);
    }
    let mut descriptions = Vec::new();
    let mut ordinary = Vec::new();
    for &index in order {
        let source = &pending_sources[index];
        let trigger = &pending[index];
        if trigger.source.as_ref() != Some(source)
            || trigger.controller != human
            || trigger.trigger_kind != crate::rl::PendingTriggerKindV2::TriggeredAbility
        {
            return Err(Error::UnsupportedPrompt);
        }
        let rows: Vec<_> = observation
            .entry_occurrences
            .iter()
            .filter(|row| row.pending_order_index == Some(index as u32))
            .collect();
        if let [row] = rows.as_slice() {
            if row.observer_at_entry != *source
                || row.beneficiary != human
                || row.lifecycle != crate::authority_public_v1::EntryLifecycleV1::Pending
            {
                return Err(Error::InvalidVisibleReference);
            }
            descriptions.push(format!(
                "Gain 1 life from {} for entry #{} of {}",
                handles.name(source)?,
                row.event_order + 1,
                handles.name(&row.entrant_at_entry)?
            ));
        } else if rows.is_empty() {
            if ordinary.contains(source) {
                return Err(Error::UnsupportedPrompt);
            }
            ordinary.push(source.clone());
            descriptions.push(format!("Trigger from {}", handles.name(source)?));
        } else {
            return Err(Error::InvalidVisibleReference);
        }
    }
    Ok(format!(
        "Put triggers on the stack from bottom to top (last resolves first): {}",
        descriptions.join("; then ")
    ))
}

struct BoundAuthorityV17 {
    authority: OwnerV17,
    expected: FastActorDecisionV1,
    binding: AuthorityDecisionBindingV17,
    public: crate::authority_public_v1::AuthorityObservationV1,
    visible: HumanDecisionAuthorityV17,
    engine_indexes: Vec<u32>,
}
pub struct HumanAuthorityProjectorV17 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundAuthorityV17>,
    accepted: Option<(HumanActionReceiptV1, OwnerV17)>,
}
impl HumanAuthorityProjectorV17 {
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
        session: &HumanAuthoritySessionV17,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionAuthorityV17, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .authority_current_decision_input_v17(expected, self.human_seat)
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
        let visible = HumanDecisionAuthorityV17 {
            schema: "mtg-kernel-human-authority-decision/v17".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundAuthorityV17 {
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
        session: &mut HumanAuthoritySessionV17,
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
            .authority_validate_bound_decision_v17(
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
