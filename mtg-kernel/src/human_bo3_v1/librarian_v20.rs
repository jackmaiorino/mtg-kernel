//! Sealed Librarian successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::librarian_public_v1::LibrarianObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::librarian_v20::LibrarianDecisionBindingV20;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV20 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV20 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV20 {}
pub struct HumanLibrarianSessionV20 {
    inner: FastActorSessionV1,
    authority: AuthorityV20,
}
#[derive(Clone)]
pub struct HumanLibrarianSnapshotV20 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanLibrarianSessionV20 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_librarian_v20()?,
            authority: AuthorityV20 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v20(&self) -> HumanLibrarianSnapshotV20 {
        HumanLibrarianSnapshotV20 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v20(&mut self, snapshot: &HumanLibrarianSnapshotV20) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .librarian_restore_v20(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::librarian_public_v1::LibrarianObservationV1, crate::rl::RlContractError>
    {
        self.inner
            .librarian_current_decision_input_v20(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Librarian public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionLibrarianV20 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanLibrarianExtensionsV20>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanLibrarianActivationV20 {
    pub source_at_payment: HumanCardRefV1,
    pub current_source: Option<HumanCardRefV1>,
    pub captured_controller: PlayerSeatV1,
    pub once_consumed_on_payment: bool,
    pub lifecycle: crate::librarian_public_v1::LifecycleV1,
    pub actual_draw_attempt: bool,
    pub original_source_received_upgrade_and_counters: bool,
    pub current_once_used: Option<bool>,
    pub current_plus_one_counters: Option<i32>,
    pub current_creature_subtype_ids: Option<Vec<u16>>,
    pub activation_order: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanLibrarianExtensionsV20 {
    pub predecessor: HumanExtensionsV9,
    pub activations: Vec<HumanLibrarianActivationV20>,
}
fn parts(observation: &LibrarianObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &LibrarianObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanLibrarianExtensionsV20>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let activations = observation
        .activations
        .iter()
        .enumerate()
        .map(|(index, row)| {
            Ok(HumanLibrarianActivationV20 {
                source_at_payment: handles.get(&row.source_at_payment)?.clone(),
                current_source: row
                    .current_source
                    .as_ref()
                    .map(|reference| handles.get(reference).cloned())
                    .transpose()?,
                captured_controller: row.captured_controller,
                once_consumed_on_payment: row.once_consumed_on_payment,
                lifecycle: row.lifecycle,
                actual_draw_attempt: row.actual_draw_attempt,
                original_source_received_upgrade_and_counters: row
                    .original_source_received_upgrade_and_counters,
                current_once_used: row.current_once_used,
                current_plus_one_counters: row.current_plus_one_counters,
                current_creature_subtype_ids: row.current_creature_subtype_ids.clone(),
                activation_order: u32::try_from(index)
                    .map_err(|_| Error::InvalidVisibleReference)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanLibrarianExtensionsV20 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            activations,
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
type ProjectedLibrarianV20 = (
    HumanVisibleStateV1<HumanLibrarianExtensionsV20>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

// V40 uses the same typed safe fields; no predecessor artifact/input admission.
pub(super) fn register_for_akroma_parts(
    observation: &crate::librarian_public_v1::LibrarianObservationV1,
    handles: &mut Handles,
) -> Result<(), Error> {
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.activations {
        handles.insert(&row.source_at_payment)?;
        if let Some(current) = &row.current_source {
            handles.insert(current)?;
        }
    }
    Ok(())
}
pub(super) fn state_for_akroma_parts(
    observation: &crate::librarian_public_v1::LibrarianObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanLibrarianExtensionsV20, Error> {
    Ok(state(observation, handles, human)?.extensions)
}
pub(super) fn label_for_akroma_parts(
    action: &ActionSemanticV1,
    observation: &crate::librarian_public_v1::LibrarianObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    super::labels::label_librarian_v20(action, &parts(observation), handles, human)
}

pub(super) fn unary_for_akroma_parts(
    extension: &HumanLibrarianExtensionsV20,
    unary: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &extension.predecessor.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.activations {
        unary
            .entry(row.source_at_payment.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    Ok(())
}
fn project_base(
    observation: &LibrarianObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedLibrarianV20, Error> {
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
    for row in &observation.activations {
        handles.insert(&row.source_at_payment)?;
        if let Some(current) = &row.current_source {
            handles.insert(current)?;
        }
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
    for row in &initial.extensions.activations {
        unary
            .entry(row.source_at_payment.handle.clone())
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
                super::labels::label_librarian_v20(action, &parts(observation), &handles, human)?,
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

struct BoundLibrarianV20 {
    authority: AuthorityV20,
    expected: FastActorDecisionV1,
    binding: LibrarianDecisionBindingV20,
    public: crate::librarian_public_v1::LibrarianObservationV1,
    visible: HumanDecisionLibrarianV20,
    engine_indexes: Vec<u32>,
}
pub struct HumanLibrarianProjectorV20 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundLibrarianV20>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV20)>,
}
impl HumanLibrarianProjectorV20 {
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
        session: &HumanLibrarianSessionV20,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionLibrarianV20, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .librarian_current_decision_input_v20(expected, self.human_seat)
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
        let visible = HumanDecisionLibrarianV20 {
            schema: "mtg-kernel-human-librarian-decision/v20".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundLibrarianV20 {
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
        session: &mut HumanLibrarianSessionV20,
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
            .librarian_validate_bound_decision_v20(
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
