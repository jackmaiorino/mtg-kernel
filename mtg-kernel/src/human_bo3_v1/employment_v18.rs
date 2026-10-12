//! Sealed Employment successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::employment_public_v1::EmploymentObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::employment_v18::EmploymentDecisionBindingV18;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV18 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV18 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV18 {}
pub struct HumanEmploymentSessionV18 {
    inner: FastActorSessionV1,
    authority: AuthorityV18,
}
#[derive(Clone)]
pub struct HumanEmploymentSnapshotV18 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanEmploymentSessionV18 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_employment_v18()?,
            authority: AuthorityV18 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v18(&self) -> HumanEmploymentSnapshotV18 {
        HumanEmploymentSnapshotV18 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v18(&mut self, snapshot: &HumanEmploymentSnapshotV18) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .employment_restore_v18(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::employment_public_v1::EmploymentObservationV1, crate::rl::RlContractError>
    {
        self.inner
            .employment_current_decision_input_v18(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Employment public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionEmploymentV18 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanEmploymentExtensionsV18>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEmploymentInstallationV18 {
    pub source: Option<HumanCardRefV1>,
    pub recipient_at_installation: HumanCardRefV1,
    pub current_recipient: Option<HumanCardRefV1>,
    pub origin: crate::employment_public_v1::OriginV1,
    pub original_controller: PlayerSeatV1,
    pub captured_controller: PlayerSeatV1,
    pub current_tapped: Option<bool>,
    pub current_has_haste: Option<bool>,
    pub expired: bool,
    pub cleanup_turn: Option<u32>,
    pub installation_order: u32,
    pub duration: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEmploymentExtensionsV18 {
    pub predecessor: HumanExtensionsV9,
    pub temporary_control_installations: Vec<HumanEmploymentInstallationV18>,
}
fn parts(observation: &EmploymentObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &EmploymentObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanEmploymentExtensionsV18>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let installations = observation
        .installations
        .iter()
        .enumerate()
        .map(|(index, row)| {
            Ok(HumanEmploymentInstallationV18 {
                recipient_at_installation: handles.get(&row.recipient)?.clone(),
                current_recipient: row
                    .current_recipient
                    .as_ref()
                    .map(|reference| handles.get(reference).cloned())
                    .transpose()?,
                source: row
                    .source
                    .as_ref()
                    .map(|reference| handles.get(reference).cloned())
                    .transpose()?,
                origin: row.origin.clone(),
                original_controller: row.original_controller,
                captured_controller: row.captured_controller,
                current_tapped: row.current_tapped,
                current_has_haste: row.current_has_haste,
                expired: row.expired,
                cleanup_turn: row.cleanup_turn,
                installation_order: u32::try_from(index)
                    .map_err(|_| Error::InvalidVisibleReference)?,
                duration: row.duration.clone(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanEmploymentExtensionsV18 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            temporary_control_installations: installations,
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
type ProjectedEmploymentV18 = (
    HumanVisibleStateV1<HumanEmploymentExtensionsV18>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &EmploymentObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedEmploymentV18, Error> {
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
    for row in &observation.installations {
        if let Some(source) = &row.source {
            handles.insert(source)?;
        }
        handles.insert(&row.recipient)?;
        if let Some(current) = &row.current_recipient {
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
    for row in &initial.extensions.temporary_control_installations {
        unary
            .entry(row.recipient_at_installation.handle.clone())
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
                super::labels::label_employment_v18(action, &parts(observation), &handles, human)?,
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

struct BoundEmploymentV18 {
    authority: AuthorityV18,
    expected: FastActorDecisionV1,
    binding: EmploymentDecisionBindingV18,
    public: crate::employment_public_v1::EmploymentObservationV1,
    visible: HumanDecisionEmploymentV18,
    engine_indexes: Vec<u32>,
}
pub struct HumanEmploymentProjectorV18 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundEmploymentV18>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV18)>,
}
impl HumanEmploymentProjectorV18 {
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
        session: &HumanEmploymentSessionV18,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionEmploymentV18, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .employment_current_decision_input_v18(expected, self.human_seat)
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
        let visible = HumanDecisionEmploymentV18 {
            schema: "mtg-kernel-human-employment-decision/v18".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundEmploymentV18 {
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
        session: &mut HumanEmploymentSessionV18,
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
            .employment_validate_bound_decision_v18(
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
